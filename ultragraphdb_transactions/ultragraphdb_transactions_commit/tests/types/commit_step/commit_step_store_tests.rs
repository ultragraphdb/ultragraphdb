/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The commit step deciding real transactions, executed on a changeset applier over the simulated store, the driver
//! applying each synced round with `apply_round`.

use oxrdf::{Literal, Term};
use ultragraphdb_protocol_wire::ConflictKind;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transactions_commit::{CommitError, RoundBounds};

use crate::utils::fixture::{
    Driver, applier, begin, begin_with, delete_literal, held, insert, insert_literal, prepare,
    read, setup, write,
};
use ultragraphdb_transactions_txn::TxnBuffers;

fn unbounded() -> RoundBounds {
    RoundBounds::new(u32::MAX, u32::MAX)
}

fn literal(value: &str) -> Term {
    Literal::new_simple_literal(value).into()
}

#[test]
fn test_a_commit_is_held_once_its_round_is_synced_and_applied() -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert("a", "knows", "b")])?;
    let mut driver = Driver::new(&applier, unbounded());
    let mut txn = begin(&applier)?;
    write(&mut txn, vec![insert("a", "knows", "c")])?;
    driver.submit(1, prepare(txn)?)?;
    assert_eq!(driver.appended.len(), 1);
    assert_eq!(held(&applier, "a", "knows")?.len(), 1);
    assert!(driver.reply(1).is_none());
    driver.sync()?;
    let version = *driver
        .reply(1)
        .context("acknowledged")?
        .outcome()
        .as_ref()
        .context("committed")?;
    assert_eq!(applier.quads().lists().applied_ts(), version);
    assert_eq!(held(&applier, "a", "knows")?.len(), 2);
    Ok(())
}

#[test]
fn test_write_skew_is_refused() -> TestResult {
    let applier = applier()?;
    let _base = setup(
        &applier,
        vec![
            insert_literal("alice", "on-call", "true"),
            insert_literal("bob", "on-call", "true"),
        ],
    )?;
    let mut driver = Driver::new(&applier, unbounded());
    let mut first = begin(&applier)?;
    let mut second = begin(&applier)?;
    for txn in [&mut first, &mut second] {
        assert_eq!(read(txn, "alice", "on-call")?, vec![literal("true")]);
        assert_eq!(read(txn, "bob", "on-call")?, vec![literal("true")]);
    }
    write(
        &mut first,
        vec![
            delete_literal("alice", "on-call", "true"),
            insert_literal("alice", "on-call", "false"),
        ],
    )?;
    write(
        &mut second,
        vec![
            delete_literal("bob", "on-call", "true"),
            insert_literal("bob", "on-call", "false"),
        ],
    )?;
    driver.submit(1, prepare(first)?)?;
    driver.submit(2, prepare(second)?)?;
    driver.sync()?;
    driver.sync()?;
    // The first commits alone at the set-up's version plus 2 (the clock reads 0); the second, in the next round, read
    // alice's list the first rewrote.
    assert_eq!(
        driver.reply(1).context("decided")?.outcome(),
        &Ok(crate::utils::fixture::version(1, 0))
    );
    assert_eq!(
        driver.reply(2).context("decided")?.outcome(),
        &Err(CommitError::conflict(ConflictKind::List))
    );
    assert_eq!(held(&applier, "alice", "on-call")?, vec![literal("false")]);
    assert_eq!(held(&applier, "bob", "on-call")?, vec![literal("true")]);
    Ok(())
}

#[test]
fn test_inserts_of_one_new_statement_conflict_and_of_different_objects_do_not() -> TestResult {
    let applier = applier()?;
    let _base = setup(&applier, vec![insert("a", "knows", "z")])?;
    let mut driver = Driver::new(&applier, unbounded());
    let txns: Vec<_> = ["b", "c", "b"]
        .iter()
        .map(|object| {
            let mut txn = begin(&applier)?;
            write(&mut txn, vec![insert("a", "knows", object)])?;
            prepare(txn)
        })
        .collect::<TestResult<Vec<_>>>()?;
    for (ticket, prepared) in txns.into_iter().enumerate() {
        driver.submit(ticket as u64, prepared)?;
    }
    driver.sync()?;
    driver.sync()?;
    // 0 alone, then 1 and 2 in the next round: 1, another object of the list, commits; 2 tested the statement 0 wrote,
    // and found `b` absent, which 0 wrote a statement of: the statement is the first kind of read in the order list,
    // statement, range, absent term that a write above its read version touches.
    assert_eq!(
        driver.reply(0).context("decided")?.outcome(),
        &Ok(crate::utils::fixture::version(1, 0))
    );
    assert_eq!(
        driver.reply(1).context("decided")?.outcome(),
        &Ok(crate::utils::fixture::version(1, 2))
    );
    assert_eq!(
        driver.reply(2).context("decided")?.outcome(),
        &Err(CommitError::conflict(ConflictKind::Statement))
    );
    assert_eq!(held(&applier, "a", "knows")?.len(), 3);
    Ok(())
}

/// An aborted transaction's buffers come back through the step's released proposals with their capacity, as a
/// committed one's come back through its apply, and the worker's next transaction runs on them.
#[test]
fn test_an_aborted_transaction_gives_its_buffers_back() -> TestResult {
    let applier = applier()?;
    let _base = setup(&applier, vec![insert_literal("x", "name", "X")])?;
    let mut driver = Driver::new(&applier, unbounded());
    let buffers = TxnBuffers::with_capacity(4_096, 64, 8_192);
    let (read_cap, record_cap) = (buffers.read_set_capacity(), buffers.record_capacity());
    let mut loser = begin_with(&applier, buffers)?;
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
    driver.submit(1, prepare(winner)?)?;
    driver.submit(2, prepare(loser)?)?;
    driver.sync()?;
    driver.sync()?;
    assert_eq!(
        driver.reply(2).map(|r| r.outcome().clone()),
        Some(Err(CommitError::conflict(ConflictKind::List)))
    );
    assert_eq!(driver.released.len(), 1);
    let (ticket, back) = driver.released.pop().context("released")?;
    assert_eq!(ticket, 2);
    assert_eq!(
        (back.read_set_capacity(), back.record_capacity()),
        (read_cap, record_cap)
    );
    let mut next = begin_with(&applier, back)?;
    write(&mut next, vec![insert("n", "knows", "b")])?;
    driver.submit(3, prepare(next)?)?;
    driver.sync()?;
    // 1 at crate::utils::fixture::version(1, 0); 2 aborted and took none; 3 alone in the next round.
    assert_eq!(
        driver.reply(1).map(|r| r.outcome().clone()),
        Some(Ok(crate::utils::fixture::version(1, 0)))
    );
    assert_eq!(
        driver.reply(3).map(|r| r.outcome().clone()),
        Some(Ok(crate::utils::fixture::version(1, 2)))
    );
    assert!(held(&applier, "l", "knows")?.is_empty());
    Ok(())
}

/// Catalog version 2 of `namespace` starting an index build of `predicate`, executed at the watermark.
fn build_in(
    applier: &crate::utils::fixture::Applier,
    namespace: u64,
    predicate: &str,
) -> TestResult<ultragraphdb_transactions_txn::Prepared> {
    let version = crate::utils::fixture::term_version(namespace, 2, predicate);
    crate::utils::fixture::catalog_proposal(applier, &version)
}

/// Two catalog versions starting index builds in different namespaces conflict on no key, and one build runs at a time:
/// the second waits in the step, set aside, until the first's end is admitted, in a round of its own, then is decided,
/// and every one commits (decided 2026-10-28).
#[test]
fn test_a_second_build_waits_for_the_first_builds_end() -> TestResult {
    use ultragraphdb_core_mapping::Changeset;
    use ultragraphdb_transactions_txn::{Prepared, TxnBuffers};
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert_literal("a", "name", "apple")])?;
    let _base = applier
        .apply(&Changeset::new(1).with(insert_literal("a", "name", "pear")))
        .context("namespace 1 holds data")?
        .commit_ts();
    applier.sequence_builds();
    let mut driver = Driver::new(&applier, unbounded());
    let mut first = begin(&applier)?;
    write(&mut first, vec![insert("x", "knows", "y")])?;
    driver.submit(1, prepare(first)?)?;
    driver.submit(2, build_in(&applier, 0, "name")?)?;
    driver.submit(3, build_in(&applier, 1, "name")?)?;
    driver.sync()?;
    assert_eq!(
        (driver.step.waiting(), driver.step.deferred()),
        (0, 1),
        "the second build waits while the first runs"
    );
    driver.sync()?;
    assert!(applier.rebuilding(), "the first build runs");
    applier.executor().run_until_idle();
    let end = applier
        .take_build_end()
        .context("the first build is done")?;
    driver.submit(4, Prepared::end_build(&applier, end, TxnBuffers::new())?)?;
    assert_eq!(
        driver.step.deferred(),
        0,
        "back in the queue once the end is admitted"
    );
    driver.sync()?;
    driver.sync()?;
    assert!(applier.rebuilding(), "the second build runs");
    let counts: Vec<u32> = driver.appended.iter().map(|r| r.transactions()).collect();
    assert_eq!(counts, vec![1, 1, 1, 1], "the end alone in its round");
    // One transaction a round, each at the version after the last: 1, then the first build 2, its end 4, then the second
    // build 3, set aside until the end was admitted.
    let replies: Vec<(u64, Result<u64, CommitError>)> = driver
        .replies
        .iter()
        .map(|r| (r.ticket(), r.outcome().clone()))
        .collect();
    assert_eq!(
        replies,
        vec![
            (1, Ok(crate::utils::fixture::version(1, 0))),
            (2, Ok(crate::utils::fixture::version(1, 2))),
            (4, Ok(crate::utils::fixture::version(1, 4))),
            (3, Ok(crate::utils::fixture::version(1, 6)))
        ]
    );
    Ok(())
}

/// A drop closes its round: the transaction waiting after it is decided in the next round (7.7).
#[test]
fn test_a_round_closes_after_a_drop() -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(
        &applier,
        vec![insert("a", "p", "b"), insert("w", "knows", "y")],
    )?;
    let mut driver = Driver::new(&applier, unbounded());
    let mut first = begin(&applier)?;
    write(&mut first, vec![insert("x", "knows", "y")])?;
    driver.submit(1, prepare(first)?)?;
    let drop = ultragraphdb_transactions_txn::Prepared::drop(
        &applier,
        crate::utils::fixture::NS,
        ultragraphdb_store_apply::DropRequest::Predicate(crate::utils::fixture::ex("p").as_ref()),
        0,
        0,
        0,
        TxnBuffers::new(),
    )
    .context("the drop prepares")?;
    driver.submit(2, drop)?;
    let mut after = begin(&applier)?;
    write(&mut after, vec![insert("z", "knows", "y")])?;
    driver.submit(3, prepare(after)?)?;
    driver.sync()?;
    driver.sync()?;
    driver.sync()?;
    let counts: Vec<u32> = driver.appended.iter().map(|r| r.transactions()).collect();
    assert_eq!(counts, vec![1, 1, 1]);
    let reply = |t: u64| {
        driver
            .reply(t)
            .context("decided")
            .map(|r| r.outcome().clone())
    };
    assert_eq!(reply(1)?, Ok(crate::utils::fixture::version(1, 0)));
    assert_eq!(
        reply(2)?,
        Ok(crate::utils::fixture::version(1, 2)),
        "the drop takes its round's last version"
    );
    // The drop removes `p` alone: the catalog is read through its version, not a key range, so a transaction writing
    // another predicate commits in the round after the drop's, at the next version.
    assert_eq!(reply(3)?, Ok(crate::utils::fixture::version(1, 4)));
    assert!(held(&applier, "a", "p")?.is_empty());
    assert_eq!(held(&applier, "z", "knows")?.len(), 1);
    Ok(())
}

/// The driver's closes and clock reach the step over the real store: a close opens the next epoch, whose first commit
/// takes sequence 0, and a tick with nothing held changes nothing.
#[test]
fn test_a_close_through_the_driver_opens_the_next_epoch() -> TestResult {
    let applier = applier()?;
    let _base = setup(&applier, vec![insert("a", "knows", "b")])?;
    let mut driver = Driver::new(&applier, unbounded());
    driver.close(1)?;
    driver.close_with(2, Vec::new(), vec![7])?;
    driver.tick(1_000)?;
    let mut txn = begin(&applier)?;
    write(&mut txn, vec![insert("c", "knows", "b")])?;
    driver.submit(1, prepare(txn)?)?;
    driver.sync()?;
    assert_eq!(
        driver.reply(1).context("decided")?.outcome(),
        &Ok(crate::utils::fixture::version(3, 0))
    );
    assert!(driver.close_requests.is_empty());
    Ok(())
}
