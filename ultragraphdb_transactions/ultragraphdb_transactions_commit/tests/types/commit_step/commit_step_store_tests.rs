/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The commit step deciding real transactions, executed on a changeset applier over the simulated store, the driver
//! applying each synced round with `apply_round`.

use oxrdf::{Literal, Term};
use ultragraphdb_protocol_wire::ConflictKind;
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
fn test_a_commit_is_held_once_its_round_is_synced_and_applied() {
    let applier = applier();
    setup(&applier, vec![insert("a", "knows", "b")]);
    let mut driver = Driver::new(&applier, unbounded());
    let mut txn = begin(&applier);
    write(&mut txn, vec![insert("a", "knows", "c")]);
    driver.submit(1, prepare(txn));
    assert_eq!(driver.appended.len(), 1);
    assert_eq!(held(&applier, "a", "knows").len(), 1);
    assert!(driver.reply(1).is_none());
    driver.sync();
    let version = *driver
        .reply(1)
        .expect("acknowledged")
        .outcome()
        .as_ref()
        .expect("committed");
    assert_eq!(applier.quads().lists().applied_ts(), version);
    assert_eq!(held(&applier, "a", "knows").len(), 2);
}

#[test]
fn test_write_skew_is_refused() {
    let applier = applier();
    setup(
        &applier,
        vec![
            insert_literal("alice", "on-call", "true"),
            insert_literal("bob", "on-call", "true"),
        ],
    );
    let mut driver = Driver::new(&applier, unbounded());
    let mut first = begin(&applier);
    let mut second = begin(&applier);
    for txn in [&mut first, &mut second] {
        assert_eq!(read(txn, "alice", "on-call"), vec![literal("true")]);
        assert_eq!(read(txn, "bob", "on-call"), vec![literal("true")]);
    }
    write(
        &mut first,
        vec![
            delete_literal("alice", "on-call", "true"),
            insert_literal("alice", "on-call", "false"),
        ],
    );
    write(
        &mut second,
        vec![
            delete_literal("bob", "on-call", "true"),
            insert_literal("bob", "on-call", "false"),
        ],
    );
    driver.submit(1, prepare(first));
    driver.submit(2, prepare(second));
    driver.sync();
    driver.sync();
    assert!(driver.reply(1).expect("decided").outcome().is_ok());
    assert_eq!(
        driver.reply(2).expect("decided").outcome(),
        &Err(CommitError::Conflict(ConflictKind::List))
    );
    assert_eq!(held(&applier, "alice", "on-call"), vec![literal("false")]);
    assert_eq!(held(&applier, "bob", "on-call"), vec![literal("true")]);
}

#[test]
fn test_inserts_of_one_new_statement_conflict_and_of_different_objects_do_not() {
    let applier = applier();
    setup(&applier, vec![insert("a", "knows", "z")]);
    let mut driver = Driver::new(&applier, unbounded());
    let txns: Vec<_> = ["b", "c", "b"]
        .iter()
        .map(|object| {
            let mut txn = begin(&applier);
            write(&mut txn, vec![insert("a", "knows", object)]);
            prepare(txn)
        })
        .collect();
    for (ticket, prepared) in txns.into_iter().enumerate() {
        driver.submit(ticket as u64, prepared);
    }
    driver.sync();
    driver.sync();
    assert!(driver.reply(0).expect("decided").outcome().is_ok());
    assert!(driver.reply(1).expect("decided").outcome().is_ok());
    assert!(matches!(
        driver.reply(2).expect("decided").outcome(),
        Err(CommitError(
            ultragraphdb_transactions_commit::CommitErrorEnum::Conflict { .. }
        ))
    ));
    assert_eq!(held(&applier, "a", "knows").len(), 3);
}

/// An aborted transaction's buffers come back through the step's released proposals with their capacity, as a
/// committed one's come back through its apply, and the worker's next transaction runs on them.
#[test]
fn test_an_aborted_transaction_gives_its_buffers_back() {
    let applier = applier();
    setup(&applier, vec![insert_literal("x", "name", "X")]);
    let mut driver = Driver::new(&applier, unbounded());
    let buffers = TxnBuffers::with_capacity(4_096, 64, 8_192);
    let (read_cap, record_cap) = (buffers.read_set_capacity(), buffers.record_capacity());
    let mut loser = begin_with(&applier, buffers);
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
    driver.submit(1, prepare(winner));
    driver.submit(2, prepare(loser));
    driver.sync();
    driver.sync();
    assert_eq!(
        driver.reply(2).map(|r| r.outcome().clone()),
        Some(Err(CommitError::Conflict(ConflictKind::List)))
    );
    assert_eq!(driver.released.len(), 1);
    let (ticket, back) = driver.released.pop().expect("released");
    assert_eq!(ticket, 2);
    assert_eq!(
        (back.read_set_capacity(), back.record_capacity()),
        (read_cap, record_cap)
    );
    let mut next = begin_with(&applier, back);
    write(&mut next, vec![insert("n", "knows", "b")]);
    driver.submit(3, prepare(next));
    driver.sync();
    assert!(driver.reply(3).is_some_and(|r| r.outcome().is_ok()));
    assert!(held(&applier, "l", "knows").is_empty());
}

/// A declaration starting an index build of `predicate` in `namespace`, executed at the watermark.
fn build_in(
    applier: &crate::utils::fixture::Applier,
    namespace: u64,
    predicate: &str,
) -> ultragraphdb_transactions_txn::Prepared {
    use ultragraphdb_core_mapping::{Change, Changeset, Declaration, PredicateDeclaration};
    use ultragraphdb_transactions_txn::{Commit, Mode, Transaction, TxnLimits};
    let mut txn = Transaction::begin(
        applier,
        namespace,
        Mode::ReadWrite,
        0,
        0,
        TxnLimits::default(),
        TxnBuffers::new(),
    )
    .expect("it begins");
    let declared = PredicateDeclaration::new(crate::utils::fixture::ex(predicate))
        .with_tokenizers(vec!["term".to_string()]);
    txn.write(
        &Changeset::new(namespace).with(Change::Declare(Declaration::Predicate(declared))),
        0,
    )
    .expect("declared");
    let Ok(Commit::ReadWrite(prepared)) = txn.commit(0, 0, &[], 0) else {
        panic!("a read-write commit");
    };
    prepared
}

/// Two declarations starting index builds in different namespaces conflict on no key, and one build runs at a time:
/// the second waits in the step, set aside, until the first's end is admitted, in a round of its own, then is decided,
/// and every one commits (decided 2026-10-28).
#[test]
fn test_a_second_build_waits_for_the_first_builds_end() {
    use ultragraphdb_core_mapping::Changeset;
    use ultragraphdb_transactions_txn::{Prepared, TxnBuffers};
    let applier = applier();
    setup(&applier, vec![insert_literal("a", "name", "apple")]);
    applier
        .apply(&Changeset::new(1).with(insert_literal("a", "name", "pear")))
        .expect("namespace 1 holds data");
    applier.sequence_builds();
    let mut driver = Driver::new(&applier, unbounded());
    let mut first = begin(&applier);
    write(&mut first, vec![insert("x", "knows", "y")]);
    driver.submit(1, prepare(first));
    driver.submit(2, build_in(&applier, 0, "name"));
    driver.submit(3, build_in(&applier, 1, "name"));
    driver.sync();
    assert_eq!(
        (driver.step.waiting(), driver.step.deferred()),
        (0, 1),
        "the second build waits while the first runs"
    );
    driver.sync();
    assert!(applier.rebuilding(), "the first build runs");
    applier.executor().run_until_idle();
    let end = applier.take_build_end().expect("the first build is done");
    driver.submit(4, Prepared::end_build(&applier, end, TxnBuffers::new()).unwrap());
    assert_eq!(driver.step.deferred(), 0, "back in the queue once the end is admitted");
    driver.sync();
    driver.sync();
    assert!(applier.rebuilding(), "the second build runs");
    let counts: Vec<u32> = driver.appended.iter().map(|r| r.transactions()).collect();
    assert_eq!(counts, vec![1, 1, 1, 1], "the end alone in its round");
    assert!(
        driver.replies.iter().all(|r| r.outcome().is_ok()),
        "{:?}",
        driver.replies
    );
}

/// A drop closes its round: the transaction waiting after it is decided in the next round (7.7).
#[test]
fn test_a_round_closes_after_a_drop() {
    let applier = applier();
    setup(
        &applier,
        vec![insert("a", "p", "b"), insert("w", "knows", "y")],
    );
    let mut driver = Driver::new(&applier, unbounded());
    let mut first = begin(&applier);
    write(&mut first, vec![insert("x", "knows", "y")]);
    driver.submit(1, prepare(first));
    let drop = ultragraphdb_transactions_txn::Prepared::drop_predicate(
        &applier,
        crate::utils::fixture::NS,
        crate::utils::fixture::ex("p").as_ref(),
        TxnBuffers::new(),
    )
    .expect("the drop prepares");
    driver.submit(2, drop);
    let mut after = begin(&applier);
    write(&mut after, vec![insert("z", "knows", "y")]);
    driver.submit(3, prepare(after));
    driver.sync();
    driver.sync();
    driver.sync();
    let counts: Vec<u32> = driver.appended.iter().map(|r| r.transactions()).collect();
    assert_eq!(counts, vec![1, 1, 1]);
    let reply = |t: u64| driver.reply(t).expect("decided").outcome().clone();
    assert_eq!(reply(1), Ok(4));
    assert_eq!(reply(2), Ok(6), "the drop takes its round's last version");
    // The drop writes `p`'s catalog entry, inside the namespace's catalog range every transaction of it reads: one
    // that read below the drop aborts as a catalog change aborts it, and its retry commits.
    assert_eq!(reply(3), Err(CommitError::Conflict(ConflictKind::Drop)));
    assert!(held(&applier, "a", "p").is_empty());
    let mut retry = begin(&applier);
    write(&mut retry, vec![insert("z", "knows", "y")]);
    driver.submit(4, prepare(retry));
    driver.sync();
    assert!(driver.reply(4).expect("decided").outcome().is_ok());
    assert_eq!(held(&applier, "z", "knows").len(), 1);
}
