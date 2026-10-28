/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The commit-rounds and transactions scenarios the commit step's rounds, versions and resolver carry: real
//! transactions on a changeset applier over the simulated store, decided by the commit step, each synced round applied
//! by the driver standing in for the pipeline.

use cucumber::{World, then, when};
use oxrdf::{Literal, Term};
use ultragraphdb_protocol_wire::ConflictKind;
use ultragraphdb_transactions_commit::{
    CommitError, CommitErrorEnum, CommitRequest, CommitStep, Input, Reply, RoundBounds,
    RoundRecord, StepOutput,
};

// The unit tests' fixture and doubles, shared by path.
mod utils {
    #[allow(dead_code)]
    #[path = "fixture.rs"]
    pub mod fixture;
    #[allow(dead_code)]
    #[path = "proposal_double.rs"]
    pub mod proposal_double;
}

use crate::utils::fixture::{
    Driver, applier, begin, delete_literal, held, insert, insert_literal, prepare, read, setup,
    write,
};
use crate::utils::proposal_double::{Reads, TestProposal, Writes, proposal};

#[derive(Debug, Default, World)]
struct Steps {
    appended: Vec<RoundRecord>,
    syncs_before_reply: Vec<usize>,
    replies: Vec<Reply>,
    held: Vec<Term>,
    decisions: Vec<Vec<Reply>>,
}

fn unbounded() -> RoundBounds {
    RoundBounds::new(u32::MAX, u32::MAX)
}

#[when("one transaction commits on an idle node")]
fn lone_commit(w: &mut Steps) {
    let applier = applier();
    setup(&applier, vec![insert("a", "knows", "b")]);
    let mut driver = Driver::new(&applier, unbounded());
    let mut txn = begin(&applier);
    write(&mut txn, vec![insert("c", "knows", "d")]);
    driver.submit(1, prepare(txn));
    assert!(driver.reply(1).is_none(), "no reply before the sync");
    driver.sync();
    w.appended = driver.appended.clone();
    w.syncs_before_reply = vec![driver.syncs];
    w.replies = driver.replies.clone();
    w.held = held(&applier, "c", "knows");
}

#[then("it forms a round of its own and is acknowledged after one log sync and its apply")]
fn alone_after_one_sync(w: &mut Steps) {
    assert_eq!(w.appended.len(), 1);
    assert_eq!(w.appended[0].transactions(), 1);
    assert_eq!(w.syncs_before_reply, vec![1]);
    assert_eq!(w.replies.len(), 1);
    assert!(w.replies[0].outcome().is_ok(), "{:?}", w.replies);
    // Applied: a transaction beginning now reads it.
    assert_eq!(w.held.len(), 1);
}

#[when("ten transactions commit while a round's sync is in progress")]
fn ten_during_a_sync(w: &mut Steps) {
    let applier = applier();
    setup(&applier, vec![insert("a", "knows", "b")]);
    let mut driver = Driver::new(&applier, unbounded());
    let mut first = begin(&applier);
    write(&mut first, vec![insert("first", "knows", "b")]);
    driver.submit(0, prepare(first));
    let txns: Vec<_> = (1..=10)
        .map(|i| {
            let mut txn = begin(&applier);
            write(&mut txn, vec![insert(&format!("s{i}"), "knows", "b")]);
            prepare(txn)
        })
        .collect();
    for (i, prepared) in txns.into_iter().enumerate() {
        driver.submit(i as u64 + 1, prepared);
    }
    assert_eq!(
        driver.appended.len(),
        1,
        "the ten wait for the sync in progress"
    );
    driver.sync();
    driver.sync();
    w.appended = driver.appended.clone();
    w.syncs_before_reply = vec![driver.syncs];
    w.replies = driver.replies.clone();
}

#[then("they are decided in the next round, written with one log append and one sync")]
fn ten_share_a_round(w: &mut Steps) {
    assert_eq!(w.appended.len(), 2);
    assert_eq!(w.appended[1].transactions(), 10);
    assert_eq!(w.appended[1].committed(), 10);
    assert_eq!(w.syncs_before_reply, vec![2]);
    assert_eq!(w.replies.len(), 11);
    assert!(
        w.replies.iter().all(|r| r.outcome().is_ok()),
        "{:?}",
        w.replies
    );
}

#[when("the commit step's clock steps back by one second between two rounds")]
fn clock_back(w: &mut Steps) {
    let applier = applier();
    setup(&applier, vec![insert("a", "knows", "b")]);
    let mut driver = Driver::new(&applier, unbounded());
    for (ticket, (subject, now_us)) in [("x", 10_000_000u64), ("y", 9_000_000)]
        .into_iter()
        .enumerate()
    {
        driver.now_us = now_us;
        let mut txn = begin(&applier);
        write(&mut txn, vec![insert(subject, "knows", "b")]);
        driver.submit(ticket as u64, prepare(txn));
        driver.sync();
    }
    w.appended = driver.appended.clone();
    w.replies = driver.replies.clone();
}

#[then("the later round's versions are greater than the earlier round's")]
fn versions_rise(w: &mut Steps) {
    assert_eq!(w.appended.len(), 2);
    let earlier = w.appended[0].last_version().expect("committed");
    let later = w.appended[1].first_version();
    assert!(later > earlier, "{later} > {earlier}");
    // The earlier round took twice its clock: 20,000,000; the later the version after it.
    assert_eq!(earlier, 20_000_000);
    assert_eq!(later, 20_000_002);
    assert_eq!(w.replies[1].outcome(), &Ok(20_000_002));
}

#[when("in one round T1 aborts, and T2's reads conflict only with what T1 would have written")]
fn aborted_write(w: &mut Steps) {
    let applier = applier();
    setup(
        &applier,
        vec![
            insert_literal("x", "name", "X"),
            insert_literal("y", "name", "Y"),
            insert_literal("z", "name", "Z"),
            insert("a", "knows", "b"),
        ],
    );
    let mut driver = Driver::new(&applier, unbounded());
    // T0 writes x; T1 read x and writes y, so it aborts; T2 read y and writes z.
    let mut t0 = begin(&applier);
    let mut t1 = begin(&applier);
    let mut t2 = begin(&applier);
    write(
        &mut t0,
        vec![
            delete_literal("x", "name", "X"),
            insert_literal("x", "name", "X0"),
        ],
    );
    read(&mut t1, "x", "name");
    write(
        &mut t1,
        vec![
            delete_literal("y", "name", "Y"),
            insert_literal("y", "name", "Y1"),
        ],
    );
    read(&mut t2, "y", "name");
    write(
        &mut t2,
        vec![
            delete_literal("z", "name", "Z"),
            insert_literal("z", "name", "Z2"),
        ],
    );
    let mut blocker = begin(&applier);
    write(&mut blocker, vec![insert("blocker", "knows", "b")]);
    driver.submit(9, prepare(blocker));
    driver.submit(0, prepare(t0));
    driver.submit(1, prepare(t1));
    driver.submit(2, prepare(t2));
    driver.sync();
    driver.sync();
    assert_eq!(driver.appended.len(), 2);
    assert_eq!(
        driver.appended[1].transactions(),
        3,
        "the three share one round"
    );
    w.replies = driver.replies.clone();
    w.held = held(&applier, "z", "name");
}

#[then("T2 commits")]
fn t2_commits(w: &mut Steps) {
    let reply = |t: u64| w.replies.iter().find(|r| r.ticket() == t).expect("decided");
    assert!(reply(0).outcome().is_ok(), "{:?}", reply(0));
    assert_eq!(
        reply(1).outcome(),
        &Err(CommitError::Conflict(ConflictKind::List))
    );
    assert!(reply(2).outcome().is_ok(), "{:?}", reply(2));
    assert_eq!(w.held, vec![Term::from(Literal::new_simple_literal("Z2"))]);
}

/// A round of twelve transactions over five keys, from the same state each time.
fn decide_round() -> Vec<Reply> {
    let mut step = CommitStep::<TestProposal>::new(unbounded(), 0);
    let mut out = StepOutput::new();
    let mut replies = Vec::new();
    let mut ask =
        |step: &mut CommitStep<TestProposal>, input, out: &mut StepOutput<TestProposal>| {
            step.step(input, 0, out);
            replies.extend(out.replies().iter().cloned());
        };
    ask(
        &mut step,
        Input::Commit(CommitRequest::new(
            0,
            proposal(0, &Reads::default(), &Writes::point(b"k0")),
        )),
        &mut out,
    );
    for t in 1..=12u64 {
        let read = [b'k', b'0' + (t % 5) as u8];
        let written = [b'k', b'0' + (t % 4) as u8];
        let p = proposal(0, &Reads::list(&read), &Writes::point(&written));
        ask(&mut step, Input::Commit(CommitRequest::new(t, p)), &mut out);
    }
    ask(&mut step, Input::Synced, &mut out);
    ask(&mut step, Input::Synced, &mut out);
    replies
}

#[when("the same round is validated twice from the same state")]
fn twice(w: &mut Steps) {
    w.decisions = vec![decide_round(), decide_round()];
}

#[then("both decide the same transactions committed and aborted")]
fn alike(w: &mut Steps) {
    assert_eq!(w.decisions[0].len(), 13);
    assert_eq!(w.decisions[0], w.decisions[1]);
    assert!(w.decisions[0].iter().any(|r| r.outcome().is_err()));
    assert!(
        w.decisions[0]
            .iter()
            .filter(|r| r.outcome().is_ok())
            .count()
            > 2
    );
}

#[when(
    "the history has been pruned above a transaction's read version and the transaction commits"
)]
fn pruned_above(w: &mut Steps) {
    let applier = applier();
    setup(&applier, vec![insert("a", "knows", "b")]);
    let mut driver = Driver::new(&applier, unbounded());
    let mut old = begin(&applier);
    write(&mut old, vec![insert("old", "knows", "b")]);
    // Two commits move the watermark past the old transaction's read version, and the oldest open read follows.
    for (ticket, subject) in [(1, "p"), (2, "q")] {
        let mut txn = begin(&applier);
        write(&mut txn, vec![insert(subject, "knows", "b")]);
        driver.submit(ticket, prepare(txn));
        driver.sync();
    }
    let watermark = driver.step.watermark();
    driver
        .step
        .step(Input::OldestRead(watermark), 0, &mut driver.out);
    driver.submit(3, prepare(old));
    driver.sync();
    w.replies = driver.replies.clone();
    w.held = held(&applier, "old", "knows");
}

#[then("the commit fails with a retryable too-old error")]
fn too_old(w: &mut Steps) {
    let reply = w.replies.iter().find(|r| r.ticket() == 3).expect("decided");
    let Err(error) = reply.outcome() else {
        panic!("the old transaction commits: {reply:?}");
    };
    assert!(
        matches!(error.0, CommitErrorEnum::TooOld { .. }),
        "{error:?}"
    );
    assert!(error.is_retryable());
    assert!(w.held.is_empty());
}

#[when(
    "two transactions each read `on-call` of `alice` and `bob`, both `true`, and each sets a different one to `false`, concurrently"
)]
fn write_skew(w: &mut Steps) {
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
        read(txn, "alice", "on-call");
        read(txn, "bob", "on-call");
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
    w.replies = driver.replies.clone();
}

#[then("one of them commits and the other aborts with a conflict")]
fn one_commits(w: &mut Steps) {
    assert_eq!(w.replies.len(), 2);
    let committed = w.replies.iter().filter(|r| r.outcome().is_ok()).count();
    let conflicts = w
        .replies
        .iter()
        .filter(
            |r| matches!(r.outcome(), Err(e) if matches!(e.0, CommitErrorEnum::Conflict { .. })),
        )
        .count();
    assert_eq!((committed, conflicts), (1, 1), "{:?}", w.replies);
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()),
    );
}
