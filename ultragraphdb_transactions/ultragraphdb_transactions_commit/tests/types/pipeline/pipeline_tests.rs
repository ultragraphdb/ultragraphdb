/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The pipeline over the write-ahead log on simulated storage and a changeset applier over the simulated store: each
//! closed round appended at the log's tail and synced, applied once durable, its replies handed out after the apply;
//! a failed sync or a refused apply stopping every later write.

use crate::utils::fixture::Refused;
use futures::executor::block_on;
use oxrdf::{Literal, Term};
use ultragraphdb_error_log::LogErrorEnum;
use ultragraphdb_protocol_wire::ConflictKind;
use ultragraphdb_store_apply::ChangesetErrorEnum;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_log::{OrderedLog, Position};
use ultragraphdb_transactions_commit::{
    CommitError, Pipeline, PipelineError, PipelineErrorEnum, Reply, RoundBounds,
};
use ultragraphdb_transactions_log::{FaultMode, SimStorage, Wal, WalConfig};

use crate::utils::fixture::{
    Applier, Log, NS, applier, begin, delete_literal, held, id_of, insert, insert_literal, prepare,
    prepare_in, read, setup, version, wal, write,
};

type Pipe<'a> = Pipeline<
    'a,
    Log,
    ultragraphdb_store_sim::SimStore,
    ultragraphdb_store_sim::SimListCache<ultragraphdb_store_posting::CachedList>,
    ultragraphdb_store_sim::SteppedExecutor,
    ultragraphdb_traits_blocks::TestBlockSource,
>;

fn unbounded() -> RoundBounds {
    RoundBounds::new(u32::MAX, u32::MAX)
}

fn pipeline(applier: &Applier) -> TestResult<(SimStorage, Pipe<'_>)> {
    let (storage, log) = wal()?;
    Ok((
        storage,
        Pipeline::new(applier, log, unbounded()).context("it opens")?,
    ))
}

/// Commits `<subject> <knows> <b>` under `ticket`.
fn commit_insert(pipe: &mut Pipe<'_>, applier: &Applier, ticket: u64, subject: &str) -> TestResult {
    let mut txn = begin(applier)?;
    write(&mut txn, vec![insert(subject, "knows", "b")])?;
    pipe.commit(ticket, prepare(txn)?, 0).context("appended")?;
    Ok(())
}

/// The replies `pipe` handed out, without their session tokens.
fn drained(pipe: &mut Pipe<'_>) -> Vec<Reply> {
    crate::utils::fixture::plain(&std::mem::take(pipe.replies_mut()))
}

/// The replies `pipe` handed out, with their session tokens.
fn drained_raw(pipe: &mut Pipe<'_>) -> Vec<Reply> {
    std::mem::take(pipe.replies_mut())
}

#[test]
fn test_a_commit_is_acknowledged_once_synced_and_applied_and_visible_after() -> TestResult {
    let applier = applier()?;
    let _base = setup(&applier, vec![insert("a", "knows", "b")])?;
    let (_storage, mut pipe) = pipeline(&applier)?;
    commit_insert(&mut pipe, &applier, 7, "c")?;
    assert!(pipe.needs_sync());
    assert!(drained(&mut pipe).is_empty(), "no reply before the sync");
    assert!(
        held(&applier, "c", "knows")?.is_empty(),
        "not visible before the apply"
    );
    assert_eq!(pipe.log().tail().0, Position::new(1));
    assert_eq!(pipe.log().committed(), Position::ZERO);
    block_on(pipe.sync(0)).context("synced")?;
    assert!(!pipe.needs_sync());
    let version = version(1, 0);
    assert_eq!(drained(&mut pipe), vec![Reply::new(7, Ok(version))]);
    assert_eq!(pipe.watermark(), version);
    assert_eq!(applier.quads().lists().applied_ts(), version);
    assert_eq!(held(&applier, "c", "knows")?.len(), 1);
    assert_eq!(pipe.log().committed(), Position::new(1));
    let mut log = pipe.into_log();
    let rounds = block_on(log.read(Position::new(1), u32::MAX)).context("read")?;
    assert_eq!(rounds.len(), 1);
    assert_eq!(rounds[0].first_version(), version);
    assert_eq!(rounds[0].transactions(), 1);
    Ok(())
}

#[test]
fn test_the_next_round_is_decided_and_appended_while_one_syncs() -> TestResult {
    let applier = applier()?;
    let _base = setup(&applier, vec![insert("a", "knows", "b")])?;
    let (_storage, mut pipe) = pipeline(&applier)?;
    commit_insert(&mut pipe, &applier, 1, "p")?;
    commit_insert(&mut pipe, &applier, 2, "q")?;
    commit_insert(&mut pipe, &applier, 3, "r")?;
    assert_eq!(pipe.log().tail().0, Position::new(1), "two wait");
    assert_eq!(pipe.step().waiting(), 2);
    block_on(pipe.sync(0)).context("synced")?;
    assert_eq!(drained(&mut pipe), vec![Reply::new(1, Ok(version(1, 0)))]);
    assert_eq!(
        pipe.log().tail().0,
        Position::new(2),
        "the next round appended"
    );
    assert!(pipe.needs_sync());
    assert_eq!(pipe.watermark(), version(1, 0));
    block_on(pipe.sync(0)).context("synced")?;
    assert_eq!(
        drained(&mut pipe),
        vec![
            Reply::new(2, Ok(version(1, 2))),
            Reply::new(3, Ok(version(1, 4)))
        ]
    );
    assert_eq!(pipe.log().committed(), Position::new(2));
    assert_eq!(pipe.watermark(), version(1, 4));
    // Nothing waits: a sync with no round appended does nothing.
    block_on(pipe.sync(0)).context("nothing to sync")?;
    assert!(drained(&mut pipe).is_empty());
    Ok(())
}

#[test]
fn test_a_failed_sync_acknowledges_nothing_and_stops_every_later_write() -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert("a", "knows", "b")])?;
    let (storage, mut pipe) = pipeline(&applier)?;
    commit_insert(&mut pipe, &applier, 1, "p")?;
    commit_insert(&mut pipe, &applier, 2, "q")?;
    storage.fail_at(storage.trace().len(), FaultMode::Before);
    let error = block_on(pipe.sync(0)).refused("the sync fails")?;
    assert!(
        matches!(&error.0, PipelineErrorEnum::Log { cause } if matches!(cause.0, LogErrorEnum::Io { .. })),
        "{error:?}"
    );
    assert_eq!(
        drained(&mut pipe),
        vec![
            Reply::new(1, Err(CommitError::unavailable())),
            Reply::new(2, Err(CommitError::unavailable()))
        ]
    );
    assert!(pipe.is_stopped());
    assert!(!pipe.needs_sync());
    assert!(held(&applier, "p", "knows")?.is_empty());
    assert!(held(&applier, "q", "knows")?.is_empty());
    commit_insert(&mut pipe, &applier, 3, "r")?;
    assert_eq!(
        drained(&mut pipe),
        vec![Reply::new(3, Err(CommitError::unavailable()))]
    );
    assert_eq!(
        pipe.buffers_mut().len(),
        3,
        "every transaction's buffers back"
    );
    block_on(pipe.sync(0)).context("nothing to sync")?;
    Ok(())
}

#[test]
fn test_a_round_the_store_refuses_withholds_its_replies_and_stops_the_pipeline() -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert("a", "knows", "b")])?;
    let (_storage, mut pipe) = pipeline(&applier)?;
    commit_insert(&mut pipe, &applier, 1, "p")?;
    commit_insert(&mut pipe, &applier, 2, "q")?;
    // The store refuses the round's batch. The sync closes and appends the next round, holding 2, before the apply is
    // refused.
    applier
        .quads()
        .store()
        .inject(ultragraphdb_store_sim::Fault::FailNextBatch);
    let error = block_on(pipe.sync(0)).refused("refused")?;
    assert!(
        matches!(&error.0, PipelineErrorEnum::Apply { cause } if matches!(cause.0, ChangesetErrorEnum::Posting(_))),
        "{error:?}"
    );
    assert_eq!(pipe.log().tail().0, Position::new(2));
    assert_eq!(
        drained(&mut pipe),
        vec![Reply::new(2, Err(CommitError::unavailable()))],
        "the refused round's reply is withheld, the next round's is unavailable"
    );
    assert!(pipe.is_stopped());
    assert!(!pipe.needs_sync(), "the next round is never synced");
    assert!(held(&applier, "p", "knows")?.is_empty());
    commit_insert(&mut pipe, &applier, 3, "r")?;
    assert_eq!(
        drained(&mut pipe),
        vec![Reply::new(3, Err(CommitError::unavailable()))]
    );
    Ok(())
}

#[test]
fn test_the_buffers_of_applied_and_aborted_transactions_come_back() -> TestResult {
    let applier = applier()?;
    let _base = setup(&applier, vec![insert_literal("x", "name", "X")])?;
    let (_storage, mut pipe) = pipeline(&applier)?;
    let mut loser = begin(&applier)?;
    let _read: Vec<Term> = read(&mut loser, "x", "name")?;
    write(&mut loser, vec![insert("l", "knows", "b")])?;
    let mut winner = begin(&applier)?;
    write(
        &mut winner,
        vec![
            delete_literal("x", "name", "X"),
            insert_literal("x", "name", "X1"),
        ],
    )?;
    pipe.commit(1, prepare(winner)?, 0)?;
    pipe.commit(2, prepare(loser)?, 0)?;
    block_on(pipe.sync(0))?;
    block_on(pipe.sync(0))?;
    // The winner alone at the set-up's version plus 2; the loser read x's list the winner rewrote.
    assert_eq!(
        drained(&mut pipe),
        vec![
            Reply::new(1, Ok(version(1, 0))),
            Reply::new(2, Err(CommitError::conflict(ConflictKind::List)))
        ]
    );
    assert_eq!(pipe.buffers_mut().len(), 2);
    assert_eq!(
        held(&applier, "x", "name")?,
        vec![Term::from(Literal::new_simple_literal("X1"))]
    );
    Ok(())
}

#[test]
fn test_pipeline_errors_name_their_cause() -> TestResult {
    let log = PipelineError::log(ultragraphdb_error_log::LogError::poisoned());
    assert!(
        log.to_string()
            .contains("the log refused or failed a round")
    );
    assert!(std::error::Error::source(&log).is_some());
    let apply = PipelineError::apply(ultragraphdb_store_apply::ChangesetError::rebuilding("p"));
    assert!(
        apply
            .to_string()
            .contains("the store refused a synced round")
    );
    assert!(std::error::Error::source(&apply).is_some());
    assert_ne!(log, apply);
    Ok(())
}

/// Concurrent first writes of an undeclared predicate both commit through the log and the store, the second admitted
/// in the round after the first's: the catalog holds the predicate's default entry, and its retry-free second write.
#[test]
fn test_concurrent_first_writes_of_a_predicate_both_commit_through_the_pipeline() -> TestResult {
    let applier = applier()?;
    let _base = setup(&applier, vec![insert("z", "seen", "score")])?;
    let score = id_of(&applier, "score")?;
    let (_storage, mut pipe) = pipeline(&applier)?;
    let mut first = begin(&applier)?;
    let mut second = begin(&applier)?;
    write(&mut first, vec![insert_literal("a", "score", "12")])?;
    write(&mut second, vec![insert_literal("b", "score", "high")])?;
    pipe.commit(1, prepare(first)?, 0)?;
    pipe.commit(2, prepare(second)?, 0)?;
    block_on(pipe.sync(0))?;
    block_on(pipe.sync(0))?;
    // Neither aborts: the first alone, the second in the next round.
    assert_eq!(
        drained(&mut pipe),
        vec![
            Reply::new(1, Ok(version(1, 0))),
            Reply::new(2, Ok(version(1, 2)))
        ]
    );
    assert!(applier.catalog(NS)?.predicate(score).is_some());
    assert_eq!(held(&applier, "b", "score")?.len(), 1);
    Ok(())
}

/// A round the log refuses, here one larger than the log's segments of one block, acknowledges none of its transactions and stops
/// the pipeline, as a failed sync does.
#[test]
fn test_a_round_the_log_refuses_acknowledges_nothing_and_stops_the_pipeline() -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert("a", "knows", "b")])?;
    let storage = SimStorage::new(1);
    let config = WalConfig::new(1, true).with_segment_blocks(std::num::NonZeroU32::MIN);
    let (log, _) = block_on(Wal::open(storage, "/wal", config)).context("opened")?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).context("it opens")?;
    let mut txn = begin(&applier)?;
    let changes = (0..200)
        .map(|i| insert(&format!("s{i}"), "knows", "b"))
        .collect();
    write(&mut txn, changes)?;
    let error = pipe.commit(1, prepare(txn)?, 0).refused("refused")?;
    assert!(
        matches!(error.0, PipelineErrorEnum::Log { .. }),
        "{error:?}"
    );
    assert_eq!(
        drained(&mut pipe),
        vec![Reply::new(1, Err(CommitError::unavailable()))]
    );
    assert!(pipe.is_stopped());
    assert!(!pipe.needs_sync());
    assert_eq!(pipe.buffers_mut().len(), 1);
    commit_insert(&mut pipe, &applier, 2, "q")?;
    assert_eq!(
        drained(&mut pipe),
        vec![Reply::new(2, Err(CommitError::unavailable()))]
    );
    Ok(())
}

/// The session table is written with the rounds the store applies, so a pipeline opened again over the store answers
/// a retry from it as the first did (7.8).
#[test]
fn test_the_session_table_survives_a_restart_with_the_store() -> TestResult {
    let applier = applier()?;
    let _base = setup(&applier, vec![insert("a", "knows", "b")])?;
    let (_, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).context("it opens")?;
    let session = 0x07_u128 << 120;
    let mut txn = begin(&applier)?;
    write(&mut txn, vec![insert("s", "knows", "b")])?;
    pipe.commit(2, prepare_in(txn, session, 1)?, 0)?;
    block_on(pipe.sync(0))?;
    let first = pipe.replies_mut().pop().context("answered")?;
    let committed = first.outcome().clone()?;
    assert_eq!(committed, version(1, 0));
    drop(pipe);
    let (_, fresh_log) = wal()?;
    let mut again: Pipe<'_> =
        Pipeline::new(&applier, fresh_log, unbounded()).context("it opens again")?;
    assert_eq!(again.step().sessions().last(session), Some((1, committed)));
    assert_eq!(again.retry(session, 1)?.version(), committed);
    let mut retry_txn = begin(&applier)?;
    write(&mut retry_txn, vec![insert("s", "knows", "b")])?;
    again.commit(3, prepare_in(retry_txn, session, 1)?, 0)?;
    let retried = again.replies_mut().pop().context("answered")?;
    assert_eq!(retried.outcome(), &Ok(committed));
    assert_eq!(retried.token(), first.token());
    assert_eq!(
        applier.quads().lists().applied_ts(),
        committed,
        "the retry applied nothing"
    );
    Ok(())
}

/// Every write takes its version from the commit step (decided 2026-10-28): a build a catalog version starts ends
/// with a swap the pipeline proposes once the build is done, decided in a round of its own, and a write after it
/// commits at the step's next version; the swap's reply goes to nobody.
#[test]
fn test_a_builds_swap_takes_its_version_from_the_step_and_a_later_write_commits() -> TestResult {
    let applier = applier()?;
    let _base = setup(&applier, vec![insert_literal("a", "name", "apple")])?;
    let (_s, mut pipe) = pipeline(&applier)?;
    // The close of epoch 1 announces version 2, which takes effect at epoch 2 as the round installing it.
    let announced = ultragraphdb_cluster_sequencer::CatalogVersion::new(
        ultragraphdb_core_ids::Namespace::new(NS),
        2,
    );
    pipe.close(
        crate::utils::fixture::close(1, vec![announced], Vec::new())?,
        0,
    )?;
    let version_two = crate::utils::fixture::term_version(NS, 2, "name");
    let refusals = pipe.install(1, &version_two, 0)?;
    assert!(refusals.is_empty(), "{refusals:?}");
    block_on(pipe.sync(0))?;
    assert!(applier.rebuilding());
    assert!(pipe.step().is_building());
    applier.executor().run_until_idle();
    assert!(applier.rebuilding(), "built, its swap not yet decided");
    pipe.poll_builds(0)?;
    assert!(!pipe.step().is_building(), "the swap is admitted");
    block_on(pipe.sync(0))?;
    assert!(!applier.rebuilding(), "swapped in");
    let swapped = pipe.watermark();
    assert_eq!(
        swapped,
        pipe.step().previous_version(),
        "at the step's version"
    );
    // The version took version(2, 0) and the swap, a round of its own, the step's next version.
    assert_eq!(swapped, version(2, 2));
    let mut txn = begin(&applier)?;
    write(&mut txn, vec![insert("b", "knows", "c")])?;
    pipe.commit(2, prepare(txn)?, 0)?;
    block_on(pipe.sync(0)).context("the write applies after the swap")?;
    assert_eq!(
        drained(&mut pipe),
        vec![
            Reply::new(1, Ok(version(2, 0))),
            Reply::new(2, Ok(version(2, 4)))
        ],
        "the swap's reply goes to nobody"
    );
    assert_eq!(pipe.watermark(), version(2, 4));
    assert_eq!(held(&applier, "b", "knows")?.len(), 1);
    Ok(())
}

/// Writes the session index `index` into `applier`'s store, as a damaged store would hold it.
fn write_index(applier: &Applier, index: Vec<u8>) -> TestResult {
    use ultragraphdb_traits_store::{PersistMode, Store, WriteBatch};
    let mut batch = WriteBatch::new();
    batch.put_versionless(
        ultragraphdb_transactions_commit::SessionEntry::INDEX_KEY.to_vec(),
        index,
    );
    applier
        .quads()
        .store()
        .apply(batch, PersistMode::Buffer)
        .context("the batch applies")?;
    Ok(())
}

/// A store whose session table is corrupted, or cannot be read, does not open a pipeline (7.8).
#[test]
fn test_a_session_table_that_does_not_read_refuses_the_pipeline() -> TestResult {
    use ultragraphdb_error_store::StoreError;
    use ultragraphdb_transactions_commit::SessionEntry;
    // Nine bytes: no whole number of 8-byte session numbers.
    let applier = applier()?;
    write_index(&applier, vec![1; 9])?;
    let (_, log) = wal()?;
    let refused = Pipeline::new(&applier, log, unbounded())
        .map(|_| ())
        .refused("refused")?;
    assert_eq!(
        refused,
        PipelineError::sessions(StoreError::corrupted(
            SessionEntry::INDEX_KEY.to_vec(),
            "a session index whose length is not a multiple of 16".to_string()
        ))
    );
    // A read the store fails is the store's own error, not corruption.
    let failing = crate::utils::fixture::applier()?;
    failing
        .quads()
        .store()
        .inject(ultragraphdb_store_sim::Fault::FailNextRead);
    let (_, failing_log) = wal()?;
    let read_failed = Pipeline::new(&failing, failing_log, unbounded())
        .map(|_| ())
        .refused("read_failed")?;
    assert_eq!(
        read_failed,
        PipelineError::sessions(StoreError::read_failed(
            SessionEntry::INDEX_KEY.to_vec(),
            "simulated read failure".to_string()
        ))
    );
    Ok(())
}

/// A build's proposal that cannot be made stops the pipeline: nothing later commits. A build's end reads nothing of
/// the store, its catalog held in memory; its start, the catalog version taking effect, reads the data its
/// declarations are checked against, and that read fails.
#[test]
fn test_a_builds_end_that_cannot_be_proposed_stops_the_pipeline() -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert_literal("a", "name", "apple")])?;
    let (_s, mut pipe) = pipeline(&applier)?;
    let narrowing = ultragraphdb_core_catalog::CatalogContents::new(
        NS,
        2,
        vec![ultragraphdb_core_mapping::Declaration::Predicate(
            ultragraphdb_core_mapping::PredicateDeclaration::new(crate::utils::fixture::ex("name"))
                .with_cardinality(ultragraphdb_core_mapping::Cardinality::Single),
        )],
    );
    applier
        .quads()
        .store()
        .inject(ultragraphdb_store_sim::Fault::FailNextScanAfter(0));
    let error = pipe
        .install(1, &narrowing, 0)
        .refused("the version cannot take effect")?;
    assert!(
        matches!(
            &error.0,
            PipelineErrorEnum::CatalogVersion { version: 2, cause }
                if matches!(cause.0, ultragraphdb_transactions_txn::TxnErrorEnum::Changeset(_))
        ),
        "{error:?}"
    );
    assert!(pipe.is_stopped());
    assert!(!pipe.needs_sync());
    commit_insert(&mut pipe, &applier, 2, "q")?;
    assert_eq!(
        drained(&mut pipe),
        vec![Reply::new(2, Err(CommitError::unavailable()))]
    );
    Ok(())
}

#[test]
fn test_the_oldest_read_prunes_the_conflict_history_up_to_the_watermark() -> TestResult {
    let applier = applier()?;
    let base = setup(&applier, vec![insert("a", "knows", "b")])?;
    let (_storage, mut pipe) = pipeline(&applier)?;
    assert!(!pipe.is_stopped(), "a pipeline opens running");
    // The history opens at the store's applied version.
    assert_eq!(pipe.step().resolver().history().floor(), base);
    commit_insert(&mut pipe, &applier, 1, "p")?;
    block_on(pipe.sync(0)).context("synced")?;
    commit_insert(&mut pipe, &applier, 2, "q")?;
    block_on(pipe.sync(0)).context("synced")?;
    assert!(!pipe.is_stopped(), "two commits stop nothing");
    // One commit a round, each two above the one before, in epoch 1.
    assert_eq!(pipe.watermark(), version(1, 2));
    pipe.oldest_read(version(1, 0));
    assert_eq!(pipe.step().resolver().history().floor(), version(1, 0));
    // A floor above the watermark is taken as the watermark (CommitStep's OldestRead).
    pipe.oldest_read(u64::MAX);
    assert_eq!(pipe.step().resolver().history().floor(), version(1, 2));
    Ok(())
}

/// A close delivered to the pipeline opens the next epoch; a full epoch's close request is handed out once.
#[test]
fn test_closes_reach_the_step_through_the_pipeline() -> TestResult {
    use ultragraphdb_core_ids::Epoch;
    let applier = applier()?;
    let _base = setup(&applier, vec![insert("a", "knows", "b")])?;
    let (_storage, pipe) = pipeline(&applier)?;
    let request =
        ultragraphdb_transactions_commit::CloseRequest::new(500, 250).context("setting")?;
    let mut pipe = pipe.with_partition(4, Epoch::new(9).context("epoch")?, request, &[]);
    commit_insert(&mut pipe, &applier, 1, "p")?;
    block_on(pipe.sync(0)).context("synced")?;
    pipe.close(crate::utils::fixture::close(10, Vec::new(), Vec::new())?, 0)?;
    assert_eq!(pipe.step().delivered(), Epoch::new(10)?);
    commit_insert(&mut pipe, &applier, 2, "q")?;
    block_on(pipe.sync(0)).context("synced")?;
    let replies = drained_raw(&mut pipe);
    let versions: Vec<Result<u64, CommitError>> =
        replies.iter().map(|r| r.outcome().clone()).collect();
    assert_eq!(versions, vec![Ok(version(10, 0)), Ok(version(11, 0))]);
    assert_eq!(
        replies
            .first()
            .and_then(|r| r.token())
            .map(|t| (t.partition(), t.epoch().get())),
        Some((4, 10))
    );
    assert_eq!(pipe.take_close_request(), None, "no epoch filled");
    pipe.tick(1_000);
    assert!(!pipe.is_stopped());
    Ok(())
}

/// A registration the partition accepted as a home goes through the pipeline's rounds and is answered with its version.
#[test]
fn test_a_registration_through_the_pipeline_is_answered_with_its_version() -> TestResult {
    let applier = applier()?;
    let _base = setup(&applier, vec![insert("a", "knows", "b")])?;
    let (_storage, mut pipe) = pipeline(&applier)?;
    let record = crate::utils::proposal_double::registration(7, 3, 1)?;
    let executed = crate::utils::fixture::nothing(&applier)?;
    pipe.registration(5, executed, record, 0)?;
    block_on(pipe.sync(0)).context("synced")?;
    let replies = drained(&mut pipe);
    assert_eq!(
        replies
            .iter()
            .map(|r| (r.ticket(), r.outcome().clone()))
            .collect::<Vec<_>>(),
        vec![(5, Ok(version(1, 0)))]
    );
    assert_eq!(
        pipe.watermark(),
        version(1, 0),
        "its round moved the watermark"
    );
    Ok(())
}

/// A pipeline whose session table holds one session refuses a second session's commit busy, naming the capacity, and
/// writes nothing of it; the first session goes on committing.
#[test]
fn test_a_pipeline_with_a_full_session_table_refuses_a_new_session_busy() -> TestResult {
    let applier = applier()?;
    let (_storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded())?.with_sessions(1);
    let mut answered = Vec::new();
    for (ticket, session, request, subject) in [(1, 7, 1, "p"), (2, 8, 1, "q"), (3, 7, 2, "r")] {
        let mut txn = begin(&applier)?;
        write(&mut txn, vec![insert(subject, "knows", "b")])?;
        pipe.commit(ticket, prepare_in(txn, session, request)?, 0)?;
        if pipe.needs_sync() {
            block_on(pipe.sync(0))?;
        }
        answered.extend(
            drained(&mut pipe)
                .iter()
                .map(|r| (r.ticket(), r.outcome().clone())),
        );
    }
    assert_eq!(
        answered
            .iter()
            .map(|(t, o)| (*t, o.is_ok()))
            .collect::<Vec<_>>(),
        vec![(1, true), (2, false), (3, true)]
    );
    assert_eq!(
        answered.get(1).map(|(_, o)| o.clone()),
        Some(Err(CommitError::busy(1)))
    );
    assert_eq!(held(&applier, "q", "knows")?, Vec::<Term>::new());
    assert_eq!(held(&applier, "r", "knows")?.len(), 1);
    Ok(())
}

/// A registration's round applies what its home's registration left at the registration's own commit version: the
/// referrer set names the member from that version on and not below it (partitions, "An entity's referrer set names
/// where its inverse edges may be"; the generation rule's first registration).
#[test]
fn test_a_registration_applies_its_member_at_its_own_commit_version() -> TestResult {
    use crate::utils::fixture::{member_at, registered};
    let applier = applier()?;
    let (_storage, mut pipe) = pipeline(&applier)?;
    let (entity, executed, record) = registered(&applier, "o", 7, 1)?;
    pipe.registration(5, executed, record, 0)?;
    block_on(pipe.sync(0)).context("synced")?;
    let replies = drained(&mut pipe);
    let at = version(1, 0);
    assert_eq!(
        replies
            .iter()
            .map(|r| (r.ticket(), r.outcome().clone()))
            .collect::<Vec<_>>(),
        vec![(5, Ok(at))]
    );
    assert_eq!(member_at(&applier, entity, 7, at)?, Some((1, 1)), "epoch 1");
    assert_eq!(member_at(&applier, entity, 7, at - 1)?, None);
    Ok(())
}

/// A round holding a commit and then a registration applies each at its own version, in round order.
#[test]
fn test_a_round_of_a_commit_and_a_registration_applies_both_at_their_versions() -> TestResult {
    use crate::utils::fixture::{member_at, registered};
    let applier = applier()?;
    let (_storage, mut pipe) = pipeline(&applier)?;
    // The first commit's round stays in flight while the next commit and the registration wait for one round.
    commit_insert(&mut pipe, &applier, 1, "a")?;
    commit_insert(&mut pipe, &applier, 2, "c")?;
    let (entity, executed, record) = registered(&applier, "o", 7, 1)?;
    pipe.registration(3, executed, record, 0)?;
    block_on(pipe.sync(0)).context("the first round synced")?;
    block_on(pipe.sync(0)).context("the second round synced")?;
    let replies = drained(&mut pipe);
    let outcomes: Vec<(u64, Result<u64, CommitError>)> = replies
        .iter()
        .map(|r| (r.ticket(), r.outcome().clone()))
        .collect();
    // Epoch 1's sequences go 0, 2, 4: each commit version is even (commit-rounds, "Commit versions are built from the
    // epoch").
    assert_eq!(
        outcomes,
        vec![
            (1, Ok(version(1, 0))),
            (2, Ok(version(1, 2))),
            (3, Ok(version(1, 4)))
        ]
    );
    assert_eq!(held(&applier, "c", "knows")?.len(), 1);
    assert_eq!(member_at(&applier, entity, 7, version(1, 4))?, Some((1, 1)));
    assert_eq!(member_at(&applier, entity, 7, version(1, 2))?, None);
    Ok(())
}

/// The ID a writer of `<name>` is given now, its proposal released at once: the ID another write holds, or a new one.
fn id_now(applier: &Applier, name: &str) -> TestResult<ultragraphdb_core_ids::SystemId> {
    let mut txn = begin(applier)?;
    write(&mut txn, vec![insert(name, "p", "q")])?;
    let prepared = prepare(txn)?;
    let id = crate::utils::fixture::id_in(&prepared, name)?;
    let _buffers = prepared.release(applier);
    Ok(id)
}

/// A registration the step refuses, its record no registration's, releases what its home held for it: a writer of the
/// IRI it created is given another ID once it is released.
#[test]
fn test_a_refused_registration_releases_what_its_home_held() -> TestResult {
    use crate::utils::fixture::registered;
    let applier = applier()?;
    let (_storage, mut pipe) = pipeline(&applier)?;
    let (entity, executed, _record) = registered(&applier, "new", 7, 1)?;
    assert_eq!(
        id_now(&applier, "new")?,
        entity,
        "the home holds the IRI it created"
    );
    pipe.registration(5, executed, vec![0; 3], 0)?;
    block_on(pipe.sync(0)).context("synced")?;
    let replies = drained(&mut pipe);
    // The wire's own reading of the three bytes is the reason the refusal names.
    let cause = ultragraphdb_protocol_wire::WriteSetRecordView::new(&[0; 3])
        .err()
        .context("three bytes are no record")?;
    assert_eq!(
        replies
            .iter()
            .map(|r| (r.ticket(), r.outcome().clone()))
            .collect::<Vec<_>>(),
        vec![(5, Err(CommitError::malformed(cause)))]
    );
    assert_ne!(id_now(&applier, "new")?, entity, "the refusal released it");
    Ok(())
}

#[test]
fn test_the_pipeline_names_the_latest_close_delivered() -> TestResult {
    let applier = applier()?;
    let (_storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded())?.with_partition(
        4,
        ultragraphdb_core_ids::Epoch::new(9)?,
        ultragraphdb_transactions_commit::CloseRequest::default(),
        &[],
    );
    assert_eq!(pipe.delivered().get(), 9);
    pipe.close(crate::utils::fixture::close(10, Vec::new(), Vec::new())?, 0)?;
    assert_eq!(pipe.delivered().get(), 10);
    Ok(())
}

/// A pipeline bound to a partition claims for it: with one partition owning every bucket, partition 1's claim commits
/// and partition 2's is refused cross-partition, naming partition 1.
#[test]
fn test_a_bound_pipeline_claims_only_its_partitions_values() -> TestResult {
    use ultragraphdb_cluster_placement::{NodeId, PartitionId, PlacementMap};
    use ultragraphdb_store_apply::Operation;
    let one = PlacementMap::even(1, NodeId::new(1))?;
    let mut outcomes = Vec::new();
    for bound in [1, 2] {
        let applier = applier()?;
        let declared = ultragraphdb_core_mapping::PredicateDeclaration::new(
            crate::utils::fixture::ex("epi.id"),
        )
        .with_unique(true);
        let _declared = setup(
            &applier,
            vec![ultragraphdb_core_mapping::Change::Declare(
                ultragraphdb_core_mapping::Declaration::Predicate(declared),
            )],
        )?;
        let (_storage, log) = wal()?;
        let mut pipe: Pipe<'_> =
            Pipeline::new(&applier, log, unbounded())?.bound(PartitionId::new(bound)?, &one);
        let mut txn = begin(&applier)?;
        txn.operate(
            Operation::InsertIfAbsent {
                graph: oxrdf::GraphName::DefaultGraph,
                predicate: crate::utils::fixture::ex("epi.id"),
                value: Literal::new_simple_literal("x").into(),
                properties: Vec::new(),
            },
            0,
        )?;
        pipe.commit(1, prepare(txn)?, 0)?;
        block_on(pipe.sync(0))?;
        outcomes.extend(drained(&mut pipe).into_iter().map(|r| r.outcome().clone()));
    }
    assert!(matches!(outcomes.first(), Some(Ok(_))), "{outcomes:?}");
    assert_eq!(
        outcomes.get(1),
        Some(&Err(CommitError::cross_partition(2, 1)))
    );
    Ok(())
}

/// A commit under a registration reply reads its mark: a renewal of `(o, 7)` committed after T's reply and before T
/// conflicts with T, which writes nothing, and T's retry marks `(o, 7)` at the renewed generation (partitions, "A
/// renewal between the reply and the commit conflicts").
#[test]
fn test_a_renewal_between_the_reply_and_the_commit_conflicts() -> TestResult {
    use std::num::NonZeroU64;
    use ultragraphdb_core_ids::{BucketGroup, Epoch};
    let applier = applier()?;
    let (_storage, mut pipe) = pipeline(&applier)?;
    let o = crate::utils::proposal_double::id(40)?;
    let g = BucketGroup::new(7)?;
    let (four, five) = (
        NonZeroU64::new(4).context("4")?,
        NonZeroU64::new(5).context("5")?,
    );
    let mut t = begin(&applier)?;
    t.mark(o, g, four, Epoch::ZERO, 0)?;
    write(&mut t, vec![insert("a", "knows", "b")])?;
    let mut renewal = begin(&applier)?;
    renewal.mark(o, g, five, Epoch::ZERO, 0)?;
    pipe.commit(1, prepare(renewal)?, 0)?;
    block_on(pipe.sync(0))?;
    pipe.commit(2, prepare(t)?, 0)?;
    block_on(pipe.sync(0))?;
    assert_eq!(
        drained(&mut pipe)
            .iter()
            .map(|r| (r.ticket(), r.outcome().clone()))
            .collect::<Vec<_>>(),
        vec![
            (1, Ok(version(1, 0))),
            (2, Err(CommitError::conflict(ConflictKind::List)))
        ]
    );
    assert!(held(&applier, "a", "knows")?.is_empty(), "T wrote nothing");
    let mut retry = begin(&applier)?;
    assert_eq!(retry.read_mark(o, g, 0)?, Some(five));
    retry.mark(o, g, five, Epoch::ZERO, 0)?;
    write(&mut retry, vec![insert("a", "knows", "b")])?;
    pipe.commit(3, prepare(retry)?, 0)?;
    block_on(pipe.sync(0))?;
    assert!(matches!(
        drained(&mut pipe).first().map(Reply::outcome),
        Some(Ok(_))
    ));
    let watermark = pipe.watermark();
    assert_eq!(
        applier.quads().mark(NS, o, g, watermark)?,
        Some(five),
        "the retry marks the renewed generation"
    );
    Ok(())
}

/// A real transaction's epoch floor holds its commit as a session token does: marked under a reply naming close 3, it is
/// answered by no round until close 3 is delivered, and then stamped at epoch 4.
#[test]
fn test_a_transactions_epoch_floor_holds_its_commit() -> TestResult {
    use std::num::NonZeroU64;
    use ultragraphdb_core_ids::{BucketGroup, Epoch, Version};
    let applier = applier()?;
    let (_storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded())?.with_partition(
        4,
        Epoch::new(1)?,
        ultragraphdb_transactions_commit::CloseRequest::default(),
        &[],
    );
    let entity = crate::utils::proposal_double::id(40)?;
    let mut txn = begin(&applier)?;
    txn.mark(
        entity,
        BucketGroup::new(7)?,
        NonZeroU64::MIN,
        Epoch::new(3)?,
        0,
    )?;
    let proposal = prepare(txn)?;
    assert_eq!(proposal.epoch_floor(), Some(Epoch::new(4)?));
    pipe.commit(1, proposal, 0)?;
    block_on(pipe.sync(0))?;
    assert!(drained(&mut pipe).is_empty(), "held for close 3");
    for epoch in [2, 3] {
        pipe.close(
            crate::utils::fixture::close(epoch, Vec::new(), Vec::new())?,
            0,
        )?;
        if pipe.needs_sync() {
            block_on(pipe.sync(0))?;
        }
    }
    let replies = drained(&mut pipe);
    let reply = replies
        .first()
        .context("answered once close 3 is delivered")?;
    let version = *reply
        .outcome()
        .as_ref()
        .map_err(|e| ultragraphdb_tool_bdd::TestError::new(e.to_string()))?;
    assert_eq!(Version::from_u64(version).epoch(), Epoch::new(4)?);
    Ok(())
}
