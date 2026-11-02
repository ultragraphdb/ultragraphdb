/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Recovery (7.9): the log's rounds above the store's applied version replayed into the store, the session table
//! with them, a build's swap built again, a store ahead of the log or below a trimmed one refused.

use crate::utils::fixture::Refused;
use futures::executor::block_on;
use oxrdf::Term;
use ultragraphdb_core_mapping::{Change, Declaration, PredicateDeclaration};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_log::Position;
use ultragraphdb_traits_store::Store;
use ultragraphdb_transactions_commit::{
    Pipeline, RecoveryErrorEnum, Replay, Reply, RoundBounds, Sessions,
};
use ultragraphdb_transactions_txn::Prepared;

use crate::utils::fixture::{
    Applier, Log, NS, applier, applier_over, begin, ex, held, insert, insert_literal, prepare,
    prepare_in, reopen_wal, setup, sim_store, uid_of, wal, write,
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

/// Commits `changes` through `pipe` under `ticket`, in a round of their own: the reply.
fn commit(
    pipe: &mut Pipe<'_>,
    applier: &Applier,
    ticket: u64,
    changes: Vec<Change>,
) -> TestResult<Reply> {
    let mut txn = begin(applier)?;
    write(&mut txn, changes)?;
    pipe.commit(ticket, prepare(txn)?, 0)?;
    block_on(pipe.sync(0))?;
    pipe.replies_mut().pop().context("answered")
}

/// Replays `log` into `applier`'s store from position `from`.
fn replay(
    applier: &Applier,
    log: &mut Log,
    from: u64,
) -> Result<Replay, ultragraphdb_transactions_commit::RecoveryError> {
    block_on(Replay::run(
        applier,
        log,
        Position::new(from),
        Sessions::DEFAULT_CAPACITY,
    ))
}

#[test]
fn test_rounds_the_store_lost_are_replayed() -> TestResult {
    let store = sim_store();
    let applier = applier_over(store.clone())?;
    let (storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded())?;
    for i in 1..=10u64 {
        let _reply: Reply = commit(
            &mut pipe,
            &applier,
            i,
            vec![insert(&format!("s{i}"), "knows", "b")],
        )?;
        if i == 7 {
            applier.quads().store().sync()?;
        }
    }
    let tail = applier.quads().lists().applied_ts();
    drop(pipe);
    drop(applier);
    store.power_loss();
    store.recover();
    let recovered = applier_over(store)?;
    let lost = recovered.quads().lists().applied_ts();
    // One commit a round from an empty store with the clock at 0: round i at 2i, so 14 synced to the store after round
    // 7 and the log's tail at 20.
    assert_eq!(
        (lost, tail),
        (14, 20),
        "rounds 8 to 10 were never synced to the store"
    );
    let (mut reopened_log, _) = reopen_wal(&storage)?;
    let replayed = replay(&recovered, &mut reopened_log, 1)?;
    assert_eq!(recovered.quads().lists().applied_ts(), tail);
    assert_eq!(replayed.applied(), tail);
    assert_eq!(replayed.rounds(), 3, "rounds 8, 9 and 10");
    for i in 1..=10 {
        assert_eq!(
            held(&recovered, &format!("s{i}"), "knows")?.len(),
            1,
            "s{i}"
        );
    }
    // Replaying again replays nothing: the store holds the log's last round.
    let (mut log_again, _) = reopen_wal(&storage)?;
    assert_eq!(replay(&recovered, &mut log_again, 1)?.rounds(), 0);
    Ok(())
}

#[test]
fn test_a_store_ahead_of_the_log_refuses_to_start() -> TestResult {
    let applier = applier()?;
    let (_storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded())?;
    let _reply: Reply = commit(&mut pipe, &applier, 1, vec![insert("a", "knows", "b")])?;
    let applied = applier.quads().lists().applied_ts();
    drop(pipe);
    // Another log, holding none of the store's rounds.
    let (_other, mut other_log) = wal()?;
    let refused = replay(&applier, &mut other_log, 1).refused("refused")?;
    assert_eq!(
        refused.0,
        RecoveryErrorEnum::StoreNotInLog { version: applied }
    );
    assert!(
        refused.to_string().contains(&applied.to_string()),
        "{refused}"
    );
    // A store written outside the log, below its first round.
    let outside = crate::utils::fixture::applier()?;
    let _version: u64 = setup(&outside, vec![insert("x", "knows", "y")])?;
    let version = outside.quads().lists().applied_ts();
    let (_outside_storage, outside_log) = wal()?;
    let mut outside_pipe: Pipe<'_> = Pipeline::new(&outside, outside_log, unbounded())?;
    let _outside_reply: Reply = commit(
        &mut outside_pipe,
        &outside,
        1,
        vec![insert("a", "knows", "b")],
    )?;
    let mut outside_written = outside_pipe.into_log();
    let fresh = applier_over(sim_store())?;
    let _fresh_version: u64 = setup(&fresh, vec![insert("x", "knows", "y")])?;
    assert_eq!(
        replay(&fresh, &mut outside_written, 1)
            .refused("refused")?
            .0,
        RecoveryErrorEnum::StoreNotInLog { version }
    );
    Ok(())
}

#[test]
fn test_an_empty_store_below_a_trimmed_log_refuses_to_start() -> TestResult {
    let applier = applier()?;
    let (_storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded())?;
    let _reply: Reply = commit(&mut pipe, &applier, 1, vec![insert("a", "knows", "b")])?;
    let _reply_2: Reply = commit(&mut pipe, &applier, 2, vec![insert("c", "knows", "b")])?;
    let mut written_log = pipe.into_log();
    let empty = applier_over(sim_store())?;
    assert_eq!(
        replay(&empty, &mut written_log, 2).refused("refused")?.0,
        RecoveryErrorEnum::Gap { checkpoint: 1 }
    );
    Ok(())
}

/// The term index of `<p>`, its declaration with the `term` tokenizer.
fn term_index(p: &str) -> Change {
    Change::Declare(Declaration::Predicate(
        PredicateDeclaration::new(ex(p)).with_tokenizers(vec!["term".to_string()]),
    ))
}

/// The subjects the `term` index of `<predicate>` lists under `word`.
fn indexed(applier: &Applier, predicate: &str, word: &str) -> TestResult<Vec<u64>> {
    use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey};
    let key = QuadKey::ValueIndex {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: uid_of(applier, predicate)?,
        tokenizer: 0x01,
        token: word.as_bytes().to_vec(),
    }
    .to_bytes();
    Ok(applier
        .quads()
        .objects(&key, applier.quads().lists().applied_ts())?
        .iter()
        .filter_map(|(o, _)| o.as_uid())
        .collect())
}

/// Every kind of round replayed into an empty store gives the store the first apply left: transactions, a session's
/// registration and its committed request with its stored reply, a build's swap built again, a drop, and a swap whose
/// build a drop cancelled, discarded.
#[test]
fn test_every_kind_of_round_replays_into_an_empty_store() -> TestResult {
    let original = applier()?;
    let (_storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&original, log, unbounded())?;
    let _reply: Reply = commit(
        &mut pipe,
        &original,
        1,
        vec![insert_literal("a", "name", "big apple")],
    )?;
    pipe.register(2, 0, 0)?;
    block_on(pipe.sync(0))?;
    let session = pipe
        .replies_mut()
        .pop()
        .context("answered")?
        .outcome()
        .clone()?;
    let mut txn = begin(&original)?;
    write(&mut txn, vec![insert("s", "knows", "b")])?;
    pipe.commit(3, prepare_in(txn, session, 1)?, 0)?;
    block_on(pipe.sync(0))?;
    let committed = pipe
        .replies_mut()
        .pop()
        .context("answered")?
        .outcome()
        .clone()?;
    // A build, swapped in.
    let _reply_4: Reply = commit(&mut pipe, &original, 4, vec![term_index("name")])?;
    original.executor().run_until_idle();
    pipe.poll_builds(0)?;
    block_on(pipe.sync(0))?;
    assert!(!original.rebuilding());
    commit_a_build_a_drop_cancels(&mut pipe, &original)?;
    let applied = original.quads().lists().applied_ts();
    let mut written_log = pipe.into_log();
    let replica = applier_over(sim_store())?;
    let replayed = replay(&replica, &mut written_log, 1)?;
    assert_eq!(replica.quads().lists().applied_ts(), applied);
    // Nine rounds, each committing: the insert, the registration, the session's commit, the declaration of `name`, its
    // swap, the insert of `c`, the declaration of `label`, the drop and the cancelled build's end.
    assert_eq!(replayed.rounds(), 9);
    assert_replica_matches(&replica, &original)?;
    // The session table, with the stored reply.
    let table = Sessions::load(replica.quads().store())?;
    assert_eq!(table.last(session), Some((1, committed)));
    let _: Option<Term> = None;
    Ok(())
}

/// Commits an insert of `c` and a declaration indexing `label`, whose build a drop of `label` then cancels: the build's
/// swap is discarded.
fn commit_a_build_a_drop_cancels(pipe: &mut Pipe<'_>, original: &Applier) -> TestResult {
    let _reply: Reply = commit(
        pipe,
        original,
        5,
        vec![insert_literal("c", "label", "pear tree")],
    )?;
    let _reply_6: Reply = commit(pipe, original, 6, vec![term_index("label")])?;
    original.executor().run_until_idle();
    let drop = Prepared::drop(
        original,
        NS,
        ultragraphdb_store_apply::DropRequest::Predicate(ex("label").as_ref()),
        0,
        0,
        0,
        Default::default(),
    )?;
    pipe.commit(7, drop, 0)?;
    block_on(pipe.sync(0))?;
    pipe.poll_builds(0)?;
    block_on(pipe.sync(0))?;
    assert!(!original.rebuilding());
    Ok(())
}

/// Asserts `replica` holds what `original` does: the session's statement, `name`'s index built again and its
/// declaration, and nothing of the dropped `label`.
fn assert_replica_matches(replica: &Applier, original: &Applier) -> TestResult {
    assert_eq!(held(replica, "s", "knows")?, held(original, "s", "knows")?);
    assert_eq!(
        indexed(replica, "name", "apple")?,
        vec![uid_of(replica, "a")?]
    );
    assert_eq!(
        indexed(replica, "name", "apple")?,
        indexed(original, "name", "apple")?
    );
    let name = |a: &Applier| -> TestResult<_> {
        Ok(a.catalog(NS)?.predicate(uid_of(a, "name")?).cloned())
    };
    assert_eq!(name(replica)?, name(original)?);
    assert!(held(replica, "c", "label")?.is_empty(), "dropped");
    assert!(
        replica
            .catalog(NS)?
            .predicate(uid_of(replica, "label")?)
            .is_none()
    );
    Ok(())
}

/// The keys `applier`'s store holds, every keyspace's, versions apart: what two stores compare by.
fn keys(applier: &Applier) -> Vec<Vec<u8>> {
    applier
        .quads()
        .store()
        .contents()
        .keys()
        .keys()
        .cloned()
        .collect()
}

/// The drops of a graph, a namespace's data, a namespace and every namespace, each decided in a round through a
/// session and replayed into an empty store, leave what they left the original; a session's owner replays with it
/// (`PROTOCOL.md` §12.2 operations 4 to 7).
#[test]
fn test_every_kind_of_drop_and_a_sessions_owner_replay() -> TestResult {
    use ultragraphdb_store_apply::DropRequest;
    let graph = ex("g");
    let requests = [
        DropRequest::Graph(oxrdf::GraphNameRef::NamedNode(graph.as_ref())),
        DropRequest::Data,
        DropRequest::Namespace,
        DropRequest::All,
    ];
    for (k, request) in requests.into_iter().enumerate() {
        let original = applier()?;
        let (_storage, log) = wal()?;
        let mut pipe: Pipe<'_> = Pipeline::new(&original, log, unbounded())?;
        let _reply: Reply = commit(
            &mut pipe,
            &original,
            1,
            vec![
                insert("a", "knows", "b"),
                Change::Insert {
                    graph: oxrdf::GraphName::from(graph.clone()),
                    subject: ex("c").into(),
                    predicate: ex("knows"),
                    object: ex("d").into(),
                },
            ],
        )?;
        // A statement of another namespace, through the log too: only a drop of every namespace removes it.
        commit_in_another_namespace(&mut pipe, &original)?;
        pipe.register(2, 7, 0)?;
        block_on(pipe.sync(0))?;
        let session = pipe
            .replies_mut()
            .pop()
            .context("answered")?
            .outcome()
            .clone()?;
        let drop = Prepared::drop(&original, NS, request, session, 1, 7, Default::default())?;
        pipe.commit(3, drop, 0)?;
        block_on(pipe.sync(0))?;
        let version = pipe
            .replies_mut()
            .pop()
            .context("answered")?
            .outcome()
            .clone()
            .context("dropped")?;
        let mut written_log = pipe.into_log();
        let replica = applier_over(sim_store())?;
        let _replayed: Replay = replay(&replica, &mut written_log, 1)?;
        assert_eq!(replica.quads().lists().applied_ts(), version, "drop {k}");
        assert_eq!(keys(&replica), keys(&original), "drop {k}");
        assert_session_replayed(&replica, session, version, k)?;
    }
    Ok(())
}

/// Commits `<x> <knows> <y>` in the namespace after the tests', through the log, its reply cleared.
fn commit_in_another_namespace(pipe: &mut Pipe<'_>, original: &Applier) -> TestResult {
    let mut other = ultragraphdb_transactions_txn::Transaction::begin(
        original,
        NS + 1,
        ultragraphdb_transactions_txn::Mode::ReadWrite,
        0,
        0,
        Default::default(),
        Default::default(),
    )?;
    other.write(
        &ultragraphdb_core_mapping::Changeset::new(NS + 1).with(insert("x", "knows", "y")),
        0,
    )?;
    pipe.commit(9, prepare(other)?, 0)?;
    block_on(pipe.sync(0))?;
    pipe.replies_mut().clear();
    Ok(())
}

/// Asserts the session table `replica` replayed holds `session`'s reply of request 1 at `version`, and its owner, 7.
fn assert_session_replayed(replica: &Applier, session: u64, version: u64, k: usize) -> TestResult {
    use ultragraphdb_transactions_commit::CommitError;
    let table = Sessions::load(replica.quads().store())?;
    assert_eq!(
        table.last(session),
        Some((1, version)),
        "drop {k}: its reply"
    );
    assert_eq!(
        table.check(session, 2, 8),
        Err(CommitError::permission_denied(session)),
        "drop {k}: the session's owner replayed"
    );
    assert_eq!(
        table.check(session, 2, 7),
        Ok(None),
        "drop {k}: and its owner served"
    );
    Ok(())
}

/// A store below the last commit version the log's checkpoint covers refuses to start, naming both versions, and
/// replays nothing.
#[test]
fn test_a_store_behind_its_checkpoint_refuses_to_start() -> TestResult {
    use ultragraphdb_traits_log::OrderedLog;
    let store = sim_store();
    let applier = applier_over(store.clone())?;
    let (storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded())?;
    let _reply: Reply = commit(&mut pipe, &applier, 1, vec![insert("a", "knows", "b")])?;
    let behind = applier.quads().lists().applied_ts();
    applier.quads().store().sync()?;
    let _reply_2: Reply = commit(&mut pipe, &applier, 2, vec![insert("c", "knows", "b")])?;
    let covered = applier.quads().lists().applied_ts();
    let mut written_log = pipe.into_log();
    let position = written_log.committed();
    block_on(written_log.trim_through(position, covered))?;
    assert_eq!(written_log.checkpoint_version(), covered);
    drop(written_log);
    drop(applier);
    store.power_loss();
    store.recover();
    let recovered = applier_over(store)?;
    assert_eq!(recovered.quads().lists().applied_ts(), behind);
    let (mut reopened_log, _) = reopen_wal(&storage)?;
    assert_eq!(
        reopened_log.checkpoint_version(),
        covered,
        "the version is durable"
    );
    let refused = replay(&recovered, &mut reopened_log, position.get() + 1).refused("refused")?;
    assert_eq!(
        refused.0,
        RecoveryErrorEnum::BehindCheckpoint {
            applied: behind,
            checkpoint: covered
        }
    );
    let text = refused.to_string();
    assert!(
        text.contains(&behind.to_string()) && text.contains(&covered.to_string()),
        "{text}"
    );
    assert_eq!(
        recovered.quads().lists().applied_ts(),
        behind,
        "nothing replayed"
    );
    Ok(())
}

/// A store at the version its log's checkpoint covers, below the first round the trimmed log still holds, starts:
/// the checkpoint covers what the log no longer holds, and the rounds after it are replayed.
#[test]
fn test_a_store_at_its_checkpoint_below_a_trimmed_log_replays_the_rest() -> TestResult {
    use ultragraphdb_traits_log::OrderedLog;
    let original = applier()?;
    let (_storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&original, log, unbounded())?;
    let _reply: Reply = commit(&mut pipe, &original, 1, vec![insert("a", "knows", "b")])?;
    let mut written_log = pipe.into_log();
    // A second store holding round 1 alone, at the version the checkpoint will cover.
    let restored = applier_over(sim_store())?;
    assert_eq!(replay(&restored, &mut written_log, 1)?.rounds(), 1);
    let covered = restored.quads().lists().applied_ts();
    let mut resumed_pipe: Pipe<'_> = Pipeline::new(&original, written_log, unbounded())?;
    let _reply_2: Reply = commit(
        &mut resumed_pipe,
        &original,
        2,
        vec![insert("c", "knows", "b")],
    )?;
    let _reply_3: Reply = commit(
        &mut resumed_pipe,
        &original,
        3,
        vec![insert("d", "knows", "b")],
    )?;
    let tail = original.quads().lists().applied_ts();
    let mut resumed_log = resumed_pipe.into_log();
    // One commit a round from an empty store with the clock at 0: rounds at 2, 4 and 6.
    assert_eq!((covered, tail), (2, 6));
    // Round 1 is at position 1; the checkpoint covers it and the store's version after it.
    block_on(resumed_log.trim_through(Position::new(1), covered))?;
    let replayed = replay(&restored, &mut resumed_log, 2)?;
    assert_eq!(replayed.rounds(), 2, "rounds 2 and 3");
    assert_eq!(replayed.applied(), tail);
    assert_eq!(held(&restored, "c", "knows")?.len(), 1);
    assert_eq!(held(&restored, "d", "knows")?.len(), 1);
    Ok(())
}

/// A store at the first commit version of a round the trimmed log holds after its checkpoint lies inside that round,
/// not below it: recovery refuses to start rather than apply the round over it.
#[test]
fn test_a_store_inside_the_first_round_after_a_checkpoint_refuses_to_start() -> TestResult {
    use ultragraphdb_traits_log::OrderedLog;
    let original = applier()?;
    // The predicate and the object exist before the three transactions begin, so none finds the other's terms absent.
    assert_eq!(setup(&original, vec![insert("a", "knows", "b")])?, 2);
    let (_storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&original, log, unbounded())?;
    // Round 1 takes ticket 1; tickets 2 and 3 wait and close round 2 together once round 1 syncs.
    for (ticket, subject) in [(1, "c"), (2, "d"), (3, "e")] {
        let mut txn = begin(&original)?;
        write(&mut txn, vec![insert(subject, "knows", "b")])?;
        pipe.commit(ticket, prepare(txn)?, 0)?;
    }
    block_on(pipe.sync(0))?;
    block_on(pipe.sync(0))?;
    // Each commit two above the one before, the clock at 0: 4, then 6 and 8.
    assert_eq!(
        pipe.replies_mut().as_slice(),
        &[
            Reply::new(1, Ok(4)),
            Reply::new(2, Ok(6)),
            Reply::new(3, Ok(8))
        ]
    );
    let mut written_log = pipe.into_log();
    block_on(written_log.trim_through(Position::new(1), 4))?;
    // A store written outside the log up to 6, round 2's first commit version: three set-ups at 2, 4 and 6.
    let inside = applier_over(sim_store())?;
    for subject in ["x", "y", "z"] {
        let _version: u64 = setup(&inside, vec![insert(subject, "knows", "b")])?;
    }
    assert_eq!(inside.quads().lists().applied_ts(), 6);
    assert_eq!(
        replay(&inside, &mut written_log, 2).refused("refused")?.0,
        RecoveryErrorEnum::StoreNotInLog { version: 6 }
    );
    Ok(())
}

/// A log holding one round, `<a> <knows> <b>` committed at 2 by a pipeline over a fresh store, and the log's storage.
fn one_round_log() -> TestResult<(ultragraphdb_transactions_log::SimStorage, Log)> {
    let original = applier()?;
    let (storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&original, log, unbounded())?;
    // The first commit after an empty store: max(0 + 2, 2 × 0) = 2.
    let reply = commit(&mut pipe, &original, 1, vec![insert("a", "knows", "b")])?;
    assert_eq!(reply, Reply::new(1, Ok(2)));
    Ok((storage, pipe.into_log()))
}

/// A store whose session table is corrupted refuses recovery before any round is replayed.
#[test]
fn test_a_corrupted_session_table_refuses_recovery() -> TestResult {
    use ultragraphdb_error_store::StoreError;
    use ultragraphdb_traits_store::{PersistMode, WriteBatch};
    use ultragraphdb_transactions_commit::SessionEntry;
    let (_storage, mut log) = one_round_log()?;
    let replica = applier_over(sim_store())?;
    let mut batch = WriteBatch::new();
    // Nine bytes: no whole number of 8-byte session numbers.
    batch.put_versionless(SessionEntry::INDEX_KEY.to_vec(), vec![1; 9]);
    replica.quads().store().apply(batch, PersistMode::Buffer)?;
    let refused = replay(&replica, &mut log, 1).refused("refused")?;
    assert_eq!(
        refused.0,
        RecoveryErrorEnum::Sessions {
            cause: StoreError::corrupted(
                SessionEntry::INDEX_KEY.to_vec(),
                "a session index whose length is not a multiple of 8".to_string()
            )
        }
    );
    assert_eq!(replica.quads().lists().applied_ts(), 0, "nothing replayed");
    Ok(())
}

/// A log whose storage fails the read refuses recovery with the log's error.
#[test]
fn test_a_log_that_cannot_be_read_refuses_recovery() -> TestResult {
    use ultragraphdb_error_log::LogErrorEnum;
    use ultragraphdb_transactions_log::FaultMode;
    let (storage, log) = one_round_log()?;
    drop(log);
    let (mut reopened_log, _) = reopen_wal(&storage)?;
    storage.fail_at(storage.trace().len(), FaultMode::Before);
    let replica = applier_over(sim_store())?;
    let refused = replay(&replica, &mut reopened_log, 1).refused("refused")?;
    assert!(
        matches!(&refused.0, RecoveryErrorEnum::Log { cause } if matches!(cause.0, LogErrorEnum::Io { .. })),
        "{refused:?}"
    );
    assert_eq!(replica.quads().lists().applied_ts(), 0, "nothing replayed");
    Ok(())
}

/// A round the log holds whose body is no Round body refuses recovery naming its position, after the rounds before it
/// were replayed.
#[test]
fn test_a_round_that_is_no_round_body_refuses_recovery_naming_it() -> TestResult {
    use ultragraphdb_traits_log::{OrderedLog, RoundRef};
    let (_storage, mut log) = one_round_log()?;
    let (tail, parent) = log.tail();
    assert_eq!(tail, Position::new(1));
    // Eight bytes of 0xff: a section table pointing far past the body.
    let garbage = [0xffu8; 8];
    let _appended: Position = log
        .append(RoundRef::new(Position::new(2), parent, 4, 1, &garbage))
        .context("the log takes any body")?;
    let _synced: Position = block_on(log.sync()).context("synced")?;
    let replica = applier_over(sim_store())?;
    let refused = replay(&replica, &mut log, 1).refused("refused")?;
    assert!(
        matches!(refused.0, RecoveryErrorEnum::Round { position: 2, .. }),
        "{refused:?}"
    );
    assert_eq!(
        replica.quads().lists().applied_ts(),
        2,
        "round 1 was replayed first"
    );
    Ok(())
}

/// A store refusing a replayed round refuses recovery naming the round's position.
#[test]
fn test_a_round_the_store_refuses_refuses_recovery_naming_it() -> TestResult {
    let (_storage, mut log) = one_round_log()?;
    let replica = applier_over(sim_store())?;
    replica
        .quads()
        .store()
        .inject(ultragraphdb_store_sim::Fault::FailNextBatch);
    let refused = replay(&replica, &mut log, 1).refused("refused")?;
    assert!(
        matches!(refused.0, RecoveryErrorEnum::Apply { position: 1, .. }),
        "{refused:?}"
    );
    assert_eq!(replica.quads().lists().applied_ts(), 0, "nothing applied");
    Ok(())
}
