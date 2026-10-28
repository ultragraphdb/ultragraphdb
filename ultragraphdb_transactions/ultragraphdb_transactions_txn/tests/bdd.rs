/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The transactions scenarios a worker's transaction carries, over a changeset applier on the simulated store. A
//! commit that is acknowledged is applied by the applier on its own; a transaction's clock is the scenario's, in
//! microseconds, so waiting 11 s is a clock 11 s later.

use cucumber::{World, then, when};
use oxrdf::{Literal, Term};
use ultragraphdb_transactions_txn::{
    Commit, Mode, Transaction, TxnBuffers, TxnError, TxnErrorEnum, TxnLimits,
};

// The unit tests' fixture, shared by path.
mod utils {
    #[allow(dead_code)]
    #[path = "fixture.rs"]
    pub mod fixture;
}

use crate::utils::fixture::{
    NS, SECOND, applier, begin, changeset, commit, ex, held, insert, insert_literal, objects,
};

#[derive(Debug, Default, World)]
struct Steps {
    read: Vec<Term>,
    later: Vec<Term>,
    errors: Vec<TxnError>,
    versions: Vec<u64>,
    ended: bool,
}

#[when("a request names a read version above the visibility watermark")]
fn read_above(w: &mut Steps) {
    let applier = applier();
    let watermark = commit(&applier, vec![insert_literal("a", "name", "A")]);
    let above = watermark + 2;
    w.versions = vec![above, watermark];
    let refused = Transaction::begin(
        &applier,
        NS,
        Mode::ReadOnly,
        above,
        0,
        TxnLimits::default(),
        TxnBuffers::new(),
    );
    w.errors = refused.err().into_iter().collect();
    // The next request on the same worker.
    let mut next = begin(&applier, Mode::ReadOnly);
    w.read = objects(&mut next, "a", "name", 0).expect("the next request is served");
}

#[then(
    "it is refused at once with an error naming the version and the watermark, and the worker serves the next request"
)]
fn refused_naming_both(w: &mut Steps) {
    let (above, watermark) = (w.versions[0], w.versions[1]);
    assert_eq!(
        w.errors,
        vec![TxnError::ReadAboveWatermark(above, watermark)]
    );
    let text = w.errors[0].to_string();
    assert!(text.contains(&above.to_string()), "{text}");
    assert!(text.contains(&watermark.to_string()), "{text}");
    assert_eq!(w.read, vec![Term::from(Literal::new_simple_literal("A"))]);
}

#[when(
    "a commit setting `<a> <name> \"A\"` is acknowledged and a transaction then begins and reads `name` of `a`"
)]
fn sees_acknowledged(w: &mut Steps) {
    let applier = applier();
    commit(&applier, vec![insert_literal("a", "name", "A")]);
    let mut txn = begin(&applier, Mode::ReadWrite);
    w.read = objects(&mut txn, "a", "name", 0).expect("the read");
}

#[then("it reads `\"A\"`")]
fn reads_a(w: &mut Steps) {
    assert_eq!(w.read, vec![Term::from(Literal::new_simple_literal("A"))]);
}

#[when(
    "a read-only transaction reads `name` of `a`, another transaction then commits a new `name` of `a`, and the read-only transaction ends"
)]
fn read_only_ends(w: &mut Steps) {
    let applier = applier();
    commit(&applier, vec![insert_literal("a", "name", "A")]);
    let mut txn = begin(&applier, Mode::ReadOnly);
    w.read = objects(&mut txn, "a", "name", 0).expect("the first read");
    commit(&applier, vec![insert_literal("a", "name", "B")]);
    w.later = objects(&mut txn, "a", "name", SECOND).expect("the read after the commit");
    match txn.commit(0, 0, &[], 2 * SECOND) {
        Ok(Commit::ReadOnly(_)) => w.ended = true,
        Ok(Commit::ReadWrite(_)) => panic!("a read-only transaction goes to the commit step"),
        Err(e) => w.errors.push(e),
    }
}

#[then("it ends without an error, having read the value from before the commit")]
fn ended_without_error(w: &mut Steps) {
    assert!(w.ended, "{:?}", w.errors);
    let before = vec![Term::from(Literal::new_simple_literal("A"))];
    assert_eq!(w.read, before);
    assert_eq!(w.later, before);
}

#[when("a transaction inserts `<a> <knows> <b>` and then reads the objects of `knows` of `a`")]
fn read_own_write(w: &mut Steps) {
    let applier = applier();
    let mut txn = begin(&applier, Mode::ReadWrite);
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)
        .expect("the insert");
    w.read = objects(&mut txn, "a", "knows", 0).expect("the read");
}

#[then("the read holds `b`")]
fn holds_b(w: &mut Steps) {
    assert_eq!(w.read, vec![Term::from(ex("b"))]);
}

#[when("a read-write transaction begins, waits 11 s, and commits")]
fn waits_eleven(w: &mut Steps) {
    let applier = applier();
    let mut txn = begin(&applier, Mode::ReadWrite);
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)
        .expect("the insert");
    if let Err(e) = txn.commit(1, 1, &[], 11 * SECOND) {
        w.errors.push(e);
    }
    w.later = held(&applier, "a", "knows");
}

#[then(
    "the commit fails with a retryable error naming the 10 s limit, and nothing it wrote is held"
)]
fn past_ten(w: &mut Steps) {
    assert_eq!(w.errors, vec![TxnError::PastLimit(10_000)]);
    assert!(w.errors[0].is_retryable());
    assert!(w.errors[0].to_string().contains("10 s"), "{}", w.errors[0]);
    assert!(matches!(w.errors[0].0, TxnErrorEnum::PastLimit { .. }));
    assert!(w.later.is_empty());
}

#[when("a transaction inserts `<a> <knows> <b>` in one request and aborts in the next")]
fn inserts_and_aborts(w: &mut Steps) {
    let applier = applier();
    let mut txn = begin(&applier, Mode::ReadWrite);
    txn.write(&changeset(vec![insert("a", "knows", "b")]), 0)
        .expect("the insert");
    let _buffers = txn.abort();
    w.later = held(&applier, "a", "knows");
}

#[then("`<a> <knows> <b>` is not held")]
fn not_held(w: &mut Steps) {
    assert!(w.later.is_empty(), "{:?}", w.later);
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()),
    );
}
