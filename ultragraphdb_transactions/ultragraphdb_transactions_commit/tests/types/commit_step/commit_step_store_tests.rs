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
    Driver, applier, begin, delete_literal, held, insert, insert_literal, prepare, read, setup,
    write,
};

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
