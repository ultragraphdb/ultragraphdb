/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The pipeline releases every proposal the commit step will not apply (transactions, "An ended transaction releases
//! what it holds"): a conflict, a refusal on arrival, a retry answered from the session table and the proposals a failed
//! sync stops are let go, and a proposal applied is not. A writer of `<only>` shows whether a proposal still holds the
//! ID it created for `<only>`: it takes that ID while the proposal holds it, and is given another once it is released.

use futures::executor::block_on;
use oxrdf::{GraphName, Literal, Term};
use ultragraphdb_core_ids::{SystemId, Version};
use ultragraphdb_protocol_wire::ConflictKind;
use ultragraphdb_store_apply::{Applied, Operation};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transactions_commit::{CommitError, Pipeline, Reply, RoundBounds};
use ultragraphdb_transactions_log::FaultMode;

use crate::utils::fixture::{
    Applier, Log, Refused, applier, begin, changeset, close, delete_literal, ex, id_in, id_of,
    insert, insert_literal, prepare, prepare_in, read, setup, version, wal, write,
};

type Pipe<'a> = Pipeline<
    'a,
    Log,
    ultragraphdb_store_sim::SimStore,
    ultragraphdb_store_sim::SimListCache<ultragraphdb_store_posting::CachedList>,
    ultragraphdb_store_sim::SteppedExecutor,
    ultragraphdb_traits_blocks::TestBlockSource,
>;

/// The session the session test commits through.
const SESSION: u128 = 0xCD << 100;

fn unbounded() -> RoundBounds {
    RoundBounds::new(u32::MAX, u32::MAX)
}

/// The ID a writer of `<only>` is given now, its proposal released at once: the ID another write holds, or a new one.
fn id_now(applier: &Applier) -> TestResult<SystemId> {
    let mut writer = begin(applier)?;
    write(&mut writer, vec![insert("only", "knows", "a")])?;
    let proposal = prepare(writer)?;
    let id = id_in(&proposal, "only")?;
    let _buffers = proposal.release(applier);
    Ok(id)
}

/// The replies `pipe` handed out, without their session tokens.
fn drained(pipe: &mut Pipe<'_>) -> Vec<Reply> {
    crate::utils::fixture::plain(&std::mem::take(pipe.replies_mut()))
}

#[test]
fn test_a_proposal_aborted_with_a_conflict_is_released() -> TestResult {
    let applier = applier()?;
    let _base = setup(&applier, vec![insert_literal("x", "name", "X")])?;
    let (_storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded())?;
    let mut loser = begin(&applier)?;
    let _read: Vec<Term> = read(&mut loser, "x", "name")?;
    write(&mut loser, vec![insert("only", "knows", "a")])?;
    let loser = prepare(loser)?;
    let created = id_in(&loser, "only")?;
    assert_eq!(id_now(&applier)?, created, "the proposal holds `<only>`");
    let mut winner = begin(&applier)?;
    write(
        &mut winner,
        vec![
            delete_literal("x", "name", "X"),
            insert_literal("x", "name", "X1"),
        ],
    )?;
    pipe.commit(1, prepare(winner)?, 0)?;
    pipe.commit(2, loser, 0)?;
    block_on(pipe.sync(0))?;
    block_on(pipe.sync(0))?;
    assert_eq!(
        drained(&mut pipe),
        vec![
            Reply::new(1, Ok(version(1, 0))),
            Reply::new(2, Err(CommitError::conflict(ConflictKind::List)))
        ]
    );
    assert_ne!(
        id_now(&applier)?,
        created,
        "the aborted proposal let go of `<only>`"
    );
    assert_eq!(pipe.buffers_mut().len(), 2, "both buffers come back");
    Ok(())
}

#[test]
fn test_a_proposal_refused_on_arrival_is_released() -> TestResult {
    let applier = applier()?;
    let (_storage, log) = wal()?;
    // A frame of 64 bytes holds no record of a new subject's statement.
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, RoundBounds::new(64, u32::MAX))?;
    let mut txn = begin(&applier)?;
    write(&mut txn, vec![insert("only", "knows", "a")])?;
    let proposal = prepare(txn)?;
    let created = id_in(&proposal, "only")?;
    pipe.commit(1, proposal, 0)?;
    let replies = drained(&mut pipe);
    assert!(
        matches!(
            replies.as_slice(),
            [reply] if matches!(
                reply.outcome(),
                Err(ultragraphdb_transactions_commit::CommitError(
                    ultragraphdb_transactions_commit::CommitErrorEnum::TooLarge { limit: 0, .. }
                ))
            )
        ),
        "refused too large, a round of 64 bytes holding no record: {replies:?}"
    );
    assert_ne!(
        id_now(&applier)?,
        created,
        "the refused proposal let go of `<only>`"
    );
    Ok(())
}

#[test]
fn test_a_retry_answered_from_the_session_table_releases_its_proposal() -> TestResult {
    let applier = applier()?;
    let (_storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded())?;
    let mut first = begin(&applier)?;
    write(&mut first, vec![insert("a", "knows", "b")])?;
    pipe.commit(1, prepare_in(first, SESSION, 7)?, 0)?;
    block_on(pipe.sync(0))?;
    // The retry of request 7 runs again and creates `<only>`; the step answers it from the session table.
    let mut retry = begin(&applier)?;
    write(&mut retry, vec![insert("only", "knows", "a")])?;
    let retry = prepare_in(retry, SESSION, 7)?;
    let created = id_in(&retry, "only")?;
    pipe.commit(2, retry, 0)?;
    block_on(pipe.sync(0))?;
    assert_eq!(
        drained(&mut pipe),
        vec![
            Reply::new(1, Ok(version(1, 0))),
            Reply::new(2, Ok(version(1, 0)))
        ],
        "the retry is answered with request 7's version"
    );
    assert_ne!(
        id_now(&applier)?,
        created,
        "the retry's proposal let go of `<only>`"
    );
    Ok(())
}

#[test]
fn test_a_failed_sync_releases_the_round_in_flight() -> TestResult {
    let applier = applier()?;
    let (storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded())?;
    let mut txn = begin(&applier)?;
    write(&mut txn, vec![insert("only", "knows", "a")])?;
    let proposal = prepare(txn)?;
    let created = id_in(&proposal, "only")?;
    pipe.commit(1, proposal, 0)?;
    assert_eq!(
        id_now(&applier)?,
        created,
        "the round in flight holds `<only>`"
    );
    storage.fail_at(storage.trace().len(), FaultMode::Before);
    let _error = block_on(pipe.sync(0)).refused("the sync fails")?;
    assert_eq!(
        drained(&mut pipe),
        vec![Reply::new(1, Err(CommitError::unavailable()))]
    );
    assert_ne!(
        id_now(&applier)?,
        created,
        "the stopped round let go of `<only>`"
    );
    Ok(())
}

#[test]
fn test_a_committed_proposal_is_not_released() -> TestResult {
    let applier = applier()?;
    let (_storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded())?;
    let mut txn = begin(&applier)?;
    write(&mut txn, vec![insert("only", "knows", "a")])?;
    let proposal = prepare(txn)?;
    let created = id_in(&proposal, "only")?;
    pipe.commit(1, proposal, 0)?;
    // Decided and in flight, the proposal keeps `<only>`: a writer takes the ID it created.
    assert_eq!(id_now(&applier)?, created);
    block_on(pipe.sync(0))?;
    assert_eq!(drained(&mut pipe), vec![Reply::new(1, Ok(version(1, 0)))]);
    assert_eq!(
        id_of(&applier, "only")?,
        created,
        "the store holds the ID the proposal created"
    );
    Ok(())
}

/// A compare-and-set of `<only>`'s `status` from "open" to "closed": `<only>` is resolved first, given an ID, and the
/// operation then refused, since no statement holds the value expected.
fn compare_and_set_only() -> Operation {
    Operation::CompareAndSet {
        graph: GraphName::DefaultGraph,
        subject: ex("only").into(),
        predicate: ex("status"),
        expected: Literal::new_simple_literal("open").into(),
        new: Literal::new_simple_literal("closed").into(),
    }
}

/// An add of 1 to `<only>`'s counter `n`, which resolves `<only>` and succeeds.
fn count_only() -> Operation {
    Operation::CounterAdd {
        graph: GraphName::DefaultGraph,
        subject: ex("only").into(),
        predicate: ex("n"),
        delta: 1,
    }
}

/// Whether some write holds a pending ID of `<only>`: two writers in a row, each released at once, take the one ID it
/// holds, and otherwise each creates one of its own.
fn only_held(applier: &Applier) -> TestResult<bool> {
    Ok(id_now(applier)? == id_now(applier)?)
}

#[test]
fn test_a_refused_compare_and_set_on_a_new_iri_holds_nothing() -> TestResult {
    let applier = applier()?;
    let (_storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded())?;
    let mut txn = begin(&applier)?;
    write(&mut txn, vec![insert("a", "knows", "b")])?;
    txn.operate(compare_and_set_only(), 0).context("asked")?;
    pipe.commit(1, prepare(txn)?, 0)?;
    block_on(pipe.sync(0))?;
    assert_eq!(
        drained(&mut pipe),
        vec![Reply::new(1, Err(CommitError::operation_failed(None)))]
    );
    assert!(
        !only_held(&applier)?,
        "the refused operation let go of the `<only>` it resolved"
    );
    Ok(())
}

/// The open epoch has two versions left: a round in flight takes the first, the next round's plain commit the last, and
/// that round's commit asking an operation waits for the next epoch. Its operations ran, then were not taken, and what
/// they resolved is released; decided again once the close is delivered, it holds `<only>` until its round is applied.
#[test]
fn test_an_operated_commit_deferred_holds_nothing_and_one_adopted_holds_until_applied() -> TestResult
{
    let applier = applier()?;
    let last = u64::from(Version::COMMIT_SEQUENCE_MAX);
    let start = applier
        .execute(&changeset(vec![insert("s", "knows", "t")]))
        .context("the set-up executes")?;
    let _start: Vec<Applied> = applier
        .apply_round(vec![(version(1, last - 4), start)])
        .context("the set-up applies near the epoch's end")?;
    let (_storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded())?;
    let txn = |subject: &str| -> TestResult<_> {
        let mut txn = begin(&applier)?;
        write(&mut txn, vec![insert(subject, "knows", "b")])?;
        Ok(txn)
    };
    pipe.commit(1, prepare(txn("x")?)?, 0)?;
    pipe.commit(2, prepare(txn("y")?)?, 0)?;
    let mut operating = txn("z")?;
    operating.operate(count_only(), 0).context("asked")?;
    pipe.commit(3, prepare(operating)?, 0)?;
    block_on(pipe.sync(0))?;
    assert_eq!(
        pipe.step().waiting(),
        1,
        "the operated commit waits for the next epoch"
    );
    assert!(
        !only_held(&applier)?,
        "its operations' `<only>` was released"
    );
    // The full epoch's last round is applied; the close opens the next, and the operated commit is decided again.
    block_on(pipe.sync(0))?;
    pipe.close(close(1, Vec::new(), Vec::new())?, 0)?;
    assert!(only_held(&applier)?, "adopted, it holds `<only>`");
    let held = id_now(&applier)?;
    block_on(pipe.sync(0))?;
    let outcomes: Vec<(u64, bool)> = drained(&mut pipe)
        .iter()
        .map(|r| (r.ticket(), r.outcome().is_ok()))
        .collect();
    assert_eq!(outcomes, vec![(1, true), (2, true), (3, true)]);
    assert_eq!(
        id_of(&applier, "only")?,
        held,
        "the store holds the ID it held"
    );
    Ok(())
}
