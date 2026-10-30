/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The pipeline over the write-ahead log on simulated storage and a changeset applier over the simulated store: each
//! closed round appended at the log's tail and synced, applied once durable, its replies handed out after the apply;
//! a failed sync or a refused apply stopping every later write.

use futures::executor::block_on;
use oxrdf::{Literal, Term};
use ultragraphdb_error_log::LogErrorEnum;
use ultragraphdb_store_apply::ChangesetErrorEnum;
use ultragraphdb_traits_log::{OrderedLog, Position};
use ultragraphdb_transactions_commit::{
    CommitError, Pipeline, PipelineError, PipelineErrorEnum, Reply, RoundBounds,
};
use ultragraphdb_transactions_log::{FaultMode, SimStorage, Wal, WalConfig};

use crate::utils::fixture::{
    Applier, Log, NS, applier, begin, delete_literal, held, insert, insert_literal, prepare,
    prepare_in, read, setup, uid_of, wal, write,
};

type Pipe<'a> = Pipeline<
    'a,
    Log,
    ultragraphdb_store_sim::SimStore,
    ultragraphdb_store_sim::SimListCache<ultragraphdb_store_posting::CachedList>,
    ultragraphdb_store_sim::SteppedExecutor,
>;

fn unbounded() -> RoundBounds {
    RoundBounds::new(u32::MAX, u32::MAX)
}

fn pipeline(applier: &Applier) -> (SimStorage, Pipe<'_>) {
    let (storage, log) = wal();
    (
        storage,
        Pipeline::new(applier, log, unbounded()).expect("it opens"),
    )
}

/// Commits `<subject> <knows> <b>` under `ticket`.
fn commit_insert(pipe: &mut Pipe<'_>, applier: &Applier, ticket: u64, subject: &str) {
    let mut txn = begin(applier);
    write(&mut txn, vec![insert(subject, "knows", "b")]);
    pipe.commit(ticket, prepare(txn), 0).expect("appended");
}

fn drained(pipe: &mut Pipe<'_>) -> Vec<Reply> {
    std::mem::take(pipe.replies_mut())
}

#[test]
fn test_a_commit_is_acknowledged_once_synced_and_applied_and_visible_after() {
    let applier = applier();
    let base = setup(&applier, vec![insert("a", "knows", "b")]);
    let (_storage, mut pipe) = pipeline(&applier);
    commit_insert(&mut pipe, &applier, 7, "c");
    assert!(pipe.needs_sync());
    assert!(drained(&mut pipe).is_empty(), "no reply before the sync");
    assert!(
        held(&applier, "c", "knows").is_empty(),
        "not visible before the apply"
    );
    assert_eq!(pipe.log().tail().0, Position::new(1));
    assert_eq!(pipe.log().committed(), Position::ZERO);
    block_on(pipe.sync(0)).expect("synced");
    assert!(!pipe.needs_sync());
    let version = base + 2;
    assert_eq!(drained(&mut pipe), vec![Reply::new(7, Ok(version))]);
    assert_eq!(pipe.watermark(), version);
    assert_eq!(applier.quads().lists().applied_ts(), version);
    assert_eq!(held(&applier, "c", "knows").len(), 1);
    assert_eq!(pipe.log().committed(), Position::new(1));
    let mut log = pipe.into_log();
    let rounds = block_on(log.read(Position::new(1), u32::MAX)).expect("read");
    assert_eq!(rounds.len(), 1);
    assert_eq!(rounds[0].first_version(), version);
    assert_eq!(rounds[0].transactions(), 1);
}

#[test]
fn test_the_next_round_is_decided_and_appended_while_one_syncs() {
    let applier = applier();
    let base = setup(&applier, vec![insert("a", "knows", "b")]);
    let (_storage, mut pipe) = pipeline(&applier);
    commit_insert(&mut pipe, &applier, 1, "p");
    commit_insert(&mut pipe, &applier, 2, "q");
    commit_insert(&mut pipe, &applier, 3, "r");
    assert_eq!(pipe.log().tail().0, Position::new(1), "two wait");
    assert_eq!(pipe.step().waiting(), 2);
    block_on(pipe.sync(0)).expect("synced");
    assert_eq!(drained(&mut pipe), vec![Reply::new(1, Ok(base + 2))]);
    assert_eq!(
        pipe.log().tail().0,
        Position::new(2),
        "the next round appended"
    );
    assert!(pipe.needs_sync());
    assert_eq!(pipe.watermark(), base + 2);
    block_on(pipe.sync(0)).expect("synced");
    assert_eq!(
        drained(&mut pipe),
        vec![Reply::new(2, Ok(base + 4)), Reply::new(3, Ok(base + 6))]
    );
    assert_eq!(pipe.log().committed(), Position::new(2));
    assert_eq!(pipe.watermark(), base + 6);
    // Nothing waits: a sync with no round appended does nothing.
    block_on(pipe.sync(0)).expect("nothing to sync");
    assert!(drained(&mut pipe).is_empty());
}

#[test]
fn test_a_failed_sync_acknowledges_nothing_and_stops_every_later_write() {
    let applier = applier();
    setup(&applier, vec![insert("a", "knows", "b")]);
    let (storage, mut pipe) = pipeline(&applier);
    commit_insert(&mut pipe, &applier, 1, "p");
    commit_insert(&mut pipe, &applier, 2, "q");
    storage.fail_at(storage.trace().len(), FaultMode::Before);
    let error = block_on(pipe.sync(0)).expect_err("the sync fails");
    assert!(
        matches!(&error.0, PipelineErrorEnum::Log { cause } if matches!(cause.0, LogErrorEnum::Io { .. })),
        "{error:?}"
    );
    assert_eq!(
        drained(&mut pipe),
        vec![
            Reply::new(1, Err(CommitError::Unavailable())),
            Reply::new(2, Err(CommitError::Unavailable()))
        ]
    );
    assert!(pipe.is_stopped());
    assert!(!pipe.needs_sync());
    assert!(held(&applier, "p", "knows").is_empty());
    assert!(held(&applier, "q", "knows").is_empty());
    commit_insert(&mut pipe, &applier, 3, "r");
    assert_eq!(
        drained(&mut pipe),
        vec![Reply::new(3, Err(CommitError::Unavailable()))]
    );
    assert_eq!(
        pipe.buffers_mut().len(),
        3,
        "every transaction's buffers back"
    );
    block_on(pipe.sync(0)).expect("nothing to sync");
}

#[test]
fn test_a_round_the_store_refuses_withholds_its_replies_and_stops_the_pipeline() {
    let applier = applier();
    setup(&applier, vec![insert("a", "knows", "b")]);
    let (_storage, mut pipe) = pipeline(&applier);
    commit_insert(&mut pipe, &applier, 1, "p");
    commit_insert(&mut pipe, &applier, 2, "q");
    // A write outside the pipeline takes the version the round was given: the store refuses the round as stale. The
    // sync closes and appends the next round, holding 2, before the apply is refused.
    setup(&applier, vec![insert("x", "knows", "b")]);
    let error = block_on(pipe.sync(0)).expect_err("refused");
    assert!(
        matches!(&error.0, PipelineErrorEnum::Apply { cause } if matches!(cause.0, ChangesetErrorEnum::Posting(_))),
        "{error:?}"
    );
    assert_eq!(pipe.log().tail().0, Position::new(2));
    assert_eq!(
        drained(&mut pipe),
        vec![Reply::new(2, Err(CommitError::Unavailable()))],
        "the refused round's reply is withheld, the next round's is unavailable"
    );
    assert!(pipe.is_stopped());
    assert!(!pipe.needs_sync(), "the next round is never synced");
    assert!(held(&applier, "p", "knows").is_empty());
    commit_insert(&mut pipe, &applier, 3, "r");
    assert_eq!(
        drained(&mut pipe),
        vec![Reply::new(3, Err(CommitError::Unavailable()))]
    );
}

#[test]
fn test_the_buffers_of_applied_and_aborted_transactions_come_back() {
    let applier = applier();
    setup(&applier, vec![insert_literal("x", "name", "X")]);
    let (_storage, mut pipe) = pipeline(&applier);
    let mut loser = begin(&applier);
    read(&mut loser, "x", "name");
    write(&mut loser, vec![insert("l", "knows", "b")]);
    let mut winner = begin(&applier);
    write(
        &mut winner,
        vec![
            delete_literal("x", "name", "X"),
            insert_literal("x", "name", "X1"),
        ],
    );
    pipe.commit(1, prepare(winner), 0).unwrap();
    pipe.commit(2, prepare(loser), 0).unwrap();
    block_on(pipe.sync(0)).unwrap();
    block_on(pipe.sync(0)).unwrap();
    let replies = drained(&mut pipe);
    assert_eq!(replies.len(), 2);
    assert!(replies[0].outcome().is_ok());
    assert!(replies[1].outcome().is_err());
    assert_eq!(pipe.buffers_mut().len(), 2);
    assert_eq!(
        held(&applier, "x", "name"),
        vec![Term::from(Literal::new_simple_literal("X1"))]
    );
}

#[test]
fn test_pipeline_errors_name_their_cause() {
    let log = PipelineError::Log(ultragraphdb_error_log::LogError::Poisoned());
    assert!(
        log.to_string()
            .contains("the log refused or failed a round")
    );
    assert!(std::error::Error::source(&log).is_some());
    let apply = PipelineError::Apply(ultragraphdb_store_apply::ChangesetError::Rebuilding("p"));
    assert!(
        apply
            .to_string()
            .contains("the store refused a synced round")
    );
    assert!(std::error::Error::source(&apply).is_some());
    assert_ne!(log, apply);
}

/// Concurrent first writes of an undeclared predicate both commit through the log and the store, the second admitted
/// in the round after the first's: the catalog holds the predicate's default entry, and its retry-free second write.
#[test]
fn test_concurrent_first_writes_of_a_predicate_both_commit_through_the_pipeline() {
    let applier = applier();
    setup(&applier, vec![insert("z", "seen", "score")]);
    let score = uid_of(&applier, "score");
    let (_storage, mut pipe) = pipeline(&applier);
    let mut first = begin(&applier);
    let mut second = begin(&applier);
    write(&mut first, vec![insert_literal("a", "score", "12")]);
    write(&mut second, vec![insert_literal("b", "score", "high")]);
    pipe.commit(1, prepare(first), 0).unwrap();
    pipe.commit(2, prepare(second), 0).unwrap();
    block_on(pipe.sync(0)).unwrap();
    block_on(pipe.sync(0)).unwrap();
    let replies = drained(&mut pipe);
    assert!(replies.iter().all(|r| r.outcome().is_ok()), "{replies:?}");
    assert!(applier.catalog(NS).unwrap().predicate(score).is_some());
    assert_eq!(held(&applier, "b", "score").len(), 1);
}

/// A round the log refuses, here one larger than the log's segments of one block, acknowledges none of its transactions and stops
/// the pipeline, as a failed sync does.
#[test]
fn test_a_round_the_log_refuses_acknowledges_nothing_and_stops_the_pipeline() {
    let applier = applier();
    setup(&applier, vec![insert("a", "knows", "b")]);
    let storage = SimStorage::new(1);
    let config = WalConfig::new(1, true).with_segment_blocks(std::num::NonZeroU32::MIN);
    let (log, _) = block_on(Wal::open(storage, "/wal", config)).expect("opened");
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).expect("it opens");
    let mut txn = begin(&applier);
    let changes = (0..200)
        .map(|i| insert(&format!("s{i}"), "knows", "b"))
        .collect();
    write(&mut txn, changes);
    let error = pipe.commit(1, prepare(txn), 0).expect_err("refused");
    assert!(
        matches!(error.0, PipelineErrorEnum::Log { .. }),
        "{error:?}"
    );
    assert_eq!(
        drained(&mut pipe),
        vec![Reply::new(1, Err(CommitError::Unavailable()))]
    );
    assert!(pipe.is_stopped());
    assert!(!pipe.needs_sync());
    assert_eq!(pipe.buffers_mut().len(), 1);
    commit_insert(&mut pipe, &applier, 2, "q");
    assert_eq!(
        drained(&mut pipe),
        vec![Reply::new(2, Err(CommitError::Unavailable()))]
    );
}

/// The session table is written with the rounds the store applies, so a pipeline opened again over the store answers
/// a retry from it as the first did, and a round of a registration alone moves the watermark to its version (7.8).
#[test]
fn test_the_session_table_survives_a_restart_with_the_store() {
    let applier = applier();
    setup(&applier, vec![insert("a", "knows", "b")]);
    let (_, log) = wal();
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).expect("it opens");
    pipe.register(1, 0, 0).unwrap();
    block_on(pipe.sync(0)).unwrap();
    let session = pipe.replies_mut().pop().unwrap().outcome().clone().unwrap();
    assert_eq!(
        pipe.watermark(),
        session,
        "the registration's round moved the watermark"
    );
    let mut txn = begin(&applier);
    write(&mut txn, vec![insert("s", "knows", "b")]);
    pipe.commit(2, prepare_in(txn, session, 1), 0).unwrap();
    block_on(pipe.sync(0)).unwrap();
    let first = pipe.replies_mut().pop().unwrap();
    let version = first.outcome().clone().unwrap();
    drop(pipe);
    let (_, log) = wal();
    let mut again: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).expect("it opens again");
    assert_eq!(again.step().sessions().last(session), Some((1, version)));
    let mut txn = begin(&applier);
    write(&mut txn, vec![insert("s", "knows", "b")]);
    again.commit(3, prepare_in(txn, session, 1), 0).unwrap();
    assert_eq!(again.replies_mut().pop(), Some(Reply::new(3, Ok(version))));
    assert_eq!(
        applier.quads().lists().applied_ts(),
        version,
        "the retry applied nothing"
    );
}

/// Every write takes its version from the commit step (decided 2026-10-28): a build the pipeline's applier runs ends
/// with a swap the pipeline proposes once the build is done, decided in a round of its own, and a write after it
/// commits at the step's next version; the swap's reply goes to nobody.
#[test]
fn test_a_builds_swap_takes_its_version_from_the_step_and_a_later_write_commits() {
    use ultragraphdb_core_mapping::{Change, Declaration, PredicateDeclaration};
    let applier = applier();
    setup(&applier, vec![insert_literal("a", "name", "apple")]);
    let (_s, mut pipe) = pipeline(&applier);
    let mut declaration = begin(&applier);
    write(
        &mut declaration,
        vec![Change::Declare(Declaration::Predicate(
            PredicateDeclaration::new(crate::utils::fixture::ex("name"))
                .with_tokenizers(vec!["term".to_string()]),
        ))],
    );
    pipe.commit(1, prepare(declaration), 0).unwrap();
    block_on(pipe.sync(0)).unwrap();
    assert!(applier.rebuilding());
    assert!(pipe.step().is_building());
    applier.executor().run_until_idle();
    assert!(applier.rebuilding(), "built, its swap not yet decided");
    pipe.poll_builds(0).unwrap();
    assert!(!pipe.step().is_building(), "the swap is admitted");
    block_on(pipe.sync(0)).unwrap();
    assert!(!applier.rebuilding(), "swapped in");
    let swapped = pipe.watermark();
    assert_eq!(
        swapped,
        pipe.step().previous_version(),
        "at the step's version"
    );
    let mut txn = begin(&applier);
    write(&mut txn, vec![insert("b", "knows", "c")]);
    pipe.commit(2, prepare(txn), 0).unwrap();
    block_on(pipe.sync(0)).expect("the write applies after the swap");
    let replies: Vec<u64> = pipe.replies_mut().drain(..).map(|r| r.ticket()).collect();
    assert_eq!(replies, vec![1, 2], "the swap's reply goes to nobody");
    assert!(pipe.watermark() > swapped);
    assert_eq!(held(&applier, "b", "knows").len(), 1);
}
