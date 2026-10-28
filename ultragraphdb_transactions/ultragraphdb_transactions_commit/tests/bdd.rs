/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The commit-rounds and transactions scenarios the commit step's rounds, versions and resolver carry: real
//! transactions on a changeset applier over the simulated store, decided by the commit step, each synced round applied
//! by the driver standing in for the pipeline.

use cucumber::{World, then, when};
use oxrdf::{GraphName, Literal, NamedNode, Term};
use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey};
use ultragraphdb_core_mapping::{Change, Declaration, PredicateDeclaration};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::ConflictKind;
use ultragraphdb_store_apply::{Operation, OperationLink};
use ultragraphdb_traits_store::Store;
use ultragraphdb_transactions_commit::{
    CommitError, CommitErrorEnum, CommitRequest, CommitStep, Input, Pipeline, Reply, RoundBounds,
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
    Applier, Driver, Log, NS, applier, begin, changeset, delete_literal, ex, held, insert,
    insert_literal, open_wal, prepare, prepare_in, read, setup, uid_of, write,
};
use crate::utils::proposal_double::{Reads, TestProposal, Writes, proposal};
use ultragraphdb_transactions_log::FaultMode;
use ultragraphdb_transactions_txn::{TxnError, TxnErrorEnum};

#[derive(Debug, Default, World)]
struct Steps {
    appended: Vec<RoundRecord>,
    syncs_before_reply: Vec<usize>,
    replies: Vec<Reply>,
    held: Vec<Term>,
    decisions: Vec<Vec<Reply>>,
    entries: Vec<PredicateDeclaration>,
    indexed: Vec<u64>,
    subject: u64,
    retry_refused: bool,
    refusal: Option<TxnError>,
    versions: Vec<u64>,
    /// The statements of a predicate held, by the predicate's statistics.
    statements: Vec<i64>,
    /// Encoded objects read back, by list.
    objects: Vec<Vec<EncodedTerm>>,
    terms: Vec<Vec<Term>>,
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

#[when(
    "two transactions of `ns` read before either commits, the first writes `<a> <score> 12` and the second `<b> <score> \"high\"`, and `score` was not declared"
)]
fn first_writes(w: &mut Steps) {
    let applier = applier();
    // `score` is a known IRI, never written as a predicate, so the catalog of `ns` holds no entry for it.
    setup(&applier, vec![insert("z", "seen", "score")]);
    let score = uid_of(&applier, "score");
    assert!(applier.catalog(NS).unwrap().predicate(score).is_none());
    let mut driver = Driver::new(&applier, unbounded());
    let mut first = begin(&applier);
    let mut second = begin(&applier);
    write(
        &mut first,
        vec![Change::Insert {
            graph: GraphName::DefaultGraph,
            subject: ex("a").into(),
            predicate: ex("score"),
            object: Literal::from(12i64).into(),
        }],
    );
    write(&mut second, vec![insert_literal("b", "score", "high")]);
    driver.submit(1, prepare(first));
    driver.submit(2, prepare(second));
    driver.sync();
    driver.sync();
    w.replies = driver.replies.clone();
    let catalog = applier.catalog(NS).unwrap();
    let snapshot = applier
        .quads()
        .store()
        .snapshot(applier.quads().lists().applied_ts())
        .unwrap();
    let stored = Catalog::load(&snapshot, NS).unwrap();
    assert_eq!(*catalog, stored, "the catalog installed is the one stored");
    w.entries = stored
        .predicates()
        .filter(|(uid, _)| *uid == score)
        .map(|(_, d)| d.clone())
        .collect();
}

#[then("both commit, and the catalog of `ns` holds `score` with the default entry once")]
fn both_first_writes_commit(w: &mut Steps) {
    assert_eq!(w.replies.len(), 2);
    assert!(
        w.replies.iter().all(|r| r.outcome().is_ok()),
        "{:?}",
        w.replies
    );
    assert_eq!(w.entries, vec![PredicateDeclaration::new(ex("score"))]);
}

type Pipe<'a> = Pipeline<
    'a,
    Log,
    ultragraphdb_store_sim::SimStore,
    ultragraphdb_store_sim::SimListCache<ultragraphdb_store_posting::CachedList>,
    ultragraphdb_store_sim::SteppedExecutor,
>;

#[when("a transaction is acknowledged and another transaction begins at any time after")]
async fn acknowledged_then_begins(w: &mut Steps) {
    let applier = applier();
    setup(&applier, vec![insert("a", "knows", "b")]);
    let (_storage, log) = open_wal().await;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).expect("it opens");
    let mut first = begin(&applier);
    write(&mut first, vec![insert("c", "knows", "d")]);
    pipe.commit(1, prepare(first), 0).expect("appended");
    // Later commits keep rounds in flight: the second transaction begins right after the acknowledgement, while the
    // next round still syncs.
    let mut later = begin(&applier);
    write(&mut later, vec![insert("e", "knows", "d")]);
    pipe.commit(2, prepare(later), 0).expect("waits");
    pipe.sync(0).await.expect("synced");
    w.replies = std::mem::take(pipe.replies_mut());
    assert!(pipe.needs_sync(), "the next round syncs");
    let mut second = begin(&applier);
    w.held = read(&mut second, "c", "knows");
}

#[then("the second transaction reads the first one's writes")]
fn reads_the_first(w: &mut Steps) {
    assert_eq!(w.replies.len(), 1);
    assert!(w.replies[0].outcome().is_ok(), "{:?}", w.replies);
    assert_eq!(w.held.len(), 1);
}

#[when("the log sync of a round fails")]
async fn sync_fails(w: &mut Steps) {
    let applier = applier();
    setup(&applier, vec![insert("a", "knows", "b")]);
    let (storage, log) = open_wal().await;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).expect("it opens");
    for (ticket, subject) in [(1, "p"), (2, "q")] {
        let mut txn = begin(&applier);
        write(&mut txn, vec![insert(subject, "knows", "b")]);
        pipe.commit(ticket, prepare(txn), 0)
            .expect("appended or waiting");
    }
    storage.fail_at(storage.trace().len(), FaultMode::Before);
    assert!(pipe.sync(0).await.is_err());
    let mut later = begin(&applier);
    write(&mut later, vec![insert("r", "knows", "b")]);
    pipe.commit(3, prepare(later), 0).expect("answered");
    w.replies = std::mem::take(pipe.replies_mut());
    w.held = ["p", "q", "r"]
        .iter()
        .flat_map(|s| held(&applier, s, "knows"))
        .collect();
}

#[then(
    "none of the round's transactions is acknowledged, and later commits are refused until restart"
)]
fn none_acknowledged(w: &mut Steps) {
    let unavailable: Vec<Reply> = (1..=3)
        .map(|t| Reply::new(t, Err(CommitError::Unavailable())))
        .collect();
    assert_eq!(w.replies, unavailable);
    assert!(w.held.is_empty());
}

/// The declaration of `<p>` with the `term` tokenizer, which starts a build when `<p>` holds data.
#[when("a transaction inserts `<a> <p> <b>` and a drop of `p` commits after its read version")]
fn write_into_drop(w: &mut Steps) {
    let applier = applier();
    setup(&applier, vec![insert("x", "p", "y"), insert("x", "q", "y")]);
    let mut driver = Driver::new(&applier, unbounded());
    let mut txn = begin(&applier);
    write(&mut txn, vec![insert("a", "p", "b")]);
    let drop = ultragraphdb_transactions_txn::Prepared::drop_predicate(
        &applier,
        NS,
        ex("p").as_ref(),
        ultragraphdb_transactions_txn::TxnBuffers::new(),
    )
    .expect("the drop prepares");
    driver.submit(1, drop);
    driver.submit(2, prepare(txn));
    driver.sync();
    driver.sync();
    w.replies = driver.replies.clone();
    w.held = held(&applier, "x", "p");
    w.held.extend(held(&applier, "a", "p"));
    w.statements = vec![statements_of(&applier, "p"), statements_of(&applier, "q")];
}

#[then("the transaction aborts, and nothing of `p` is held after the drop")]
fn dropped_nothing_held(w: &mut Steps) {
    let reply = |t: u64| w.replies.iter().find(|r| r.ticket() == t).expect("decided");
    assert!(reply(1).outcome().is_ok(), "{:?}", w.replies);
    assert_eq!(
        reply(2).outcome(),
        &Err(CommitError::Conflict(ConflictKind::Drop))
    );
    assert!(w.held.is_empty(), "{:?}", w.held);
    assert_eq!(w.statements, vec![0, 1], "p dropped, q kept");
}

fn term_index(p: &str) -> Change {
    Change::Declare(Declaration::Predicate(
        PredicateDeclaration::new(ex(p)).with_tokenizers(vec!["term".to_string()]),
    ))
}

/// The subjects the `term` index of `<predicate>` lists under `word`, at the watermark.
fn indexed_term(applier: &Applier, predicate: &str, word: &str) -> Vec<u64> {
    let key = QuadKey::ValueIndex {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: uid_of(applier, predicate),
        // Dgraph's identifier of `term` (`tok/tok.go`).
        tokenizer: 0x01,
        token: word.as_bytes().to_vec(),
    }
    .to_bytes();
    applier
        .quads()
        .objects(&key, applier.quads().lists().applied_ts())
        .expect("the index reads")
        .iter()
        .filter_map(|(o, _)| o.as_uid())
        .collect()
}

#[when(
    "a round admits a declaration of `ns` that starts an index build of `p`, and a later transaction of the same round, executed against the catalog before the declaration, writes `p` in `ns`"
)]
fn build_then_write(w: &mut Steps) {
    let applier = applier();
    setup(&applier, vec![insert_literal("a", "p", "apple")]);
    let mut driver = Driver::new(&applier, unbounded());
    let mut declaration = begin(&applier);
    write(&mut declaration, vec![term_index("p")]);
    let mut later = begin(&applier);
    write(&mut later, vec![insert_literal("c", "p", "pear")]);
    // A round in flight, so the declaration and the later write share the next one.
    let mut blocker = begin(&applier);
    write(&mut blocker, vec![insert("blocker", "knows", "b")]);
    driver.submit(9, prepare(blocker));
    driver.submit(1, prepare(declaration));
    driver.submit(2, prepare(later));
    driver.sync();
    driver.sync();
    assert_eq!(driver.appended[1].transactions(), 2, "one round");
    assert!(applier.rebuilding(), "the declaration started the build");
    let mut retry = begin(&applier);
    write(&mut retry, vec![insert_literal("c", "p", "pear")]);
    driver.submit(3, prepare(retry));
    driver.sync();
    applier.executor().run_until_idle();
    assert!(!applier.rebuilding());
    w.replies = driver.replies.clone();
    w.indexed = indexed_term(&applier, "p", "pear");
    w.subject = uid_of(&applier, "c");
}

#[then("the later transaction aborts with a conflict, and its retry is indexed by the build")]
fn later_aborts(w: &mut Steps) {
    let reply = |t: u64| w.replies.iter().find(|r| r.ticket() == t).expect("decided");
    assert!(reply(1).outcome().is_ok(), "{:?}", reply(1));
    assert_eq!(
        reply(2).outcome(),
        &Err(CommitError::Conflict(ConflictKind::Catalog))
    );
    assert!(reply(3).outcome().is_ok(), "{:?}", reply(3));
    assert_eq!(w.indexed, vec![w.subject]);
}

#[when(
    "a transaction executes against the catalog of `ns`, and a declaration of `ns` commits before it"
)]
fn old_catalog(w: &mut Steps) {
    let applier = applier();
    setup(&applier, vec![insert("z", "seen", "age")]);
    let mut driver = Driver::new(&applier, unbounded());
    let mut stale = begin(&applier);
    write(&mut stale, vec![insert_literal("c", "age", "old")]);
    let mut declaration = begin(&applier);
    let integer = PredicateDeclaration::new(ex("age")).with_datatype(NamedNode::new_unchecked(
        "http://www.w3.org/2001/XMLSchema#integer",
    ));
    write(
        &mut declaration,
        vec![Change::Declare(Declaration::Predicate(integer))],
    );
    driver.submit(1, prepare(declaration));
    driver.submit(2, prepare(stale));
    driver.sync();
    driver.sync();
    w.replies = driver.replies.clone();
    let mut retry = begin(&applier);
    let changes = changeset(vec![insert_literal("c", "age", "old")]);
    w.retry_refused = retry.write(&changes, 0).is_err();
}

#[then(
    "the transaction aborts with a conflict, and its retry is checked against the new declaration"
)]
fn stale_aborts(w: &mut Steps) {
    let reply = |t: u64| w.replies.iter().find(|r| r.ticket() == t).expect("decided");
    assert!(reply(1).outcome().is_ok(), "{:?}", reply(1));
    assert_eq!(
        reply(2).outcome(),
        &Err(CommitError::Conflict(ConflictKind::Catalog))
    );
    assert!(w.retry_refused, "a string is no integer");
}

#[when(
    "a term index on `name` is being built and a transaction inserting `<c> <name> \"pear\"` commits during the build"
)]
fn during_build(w: &mut Steps) {
    let applier = applier();
    setup(&applier, vec![insert_literal("a", "name", "apple")]);
    let mut driver = Driver::new(&applier, unbounded());
    let mut declaration = begin(&applier);
    write(&mut declaration, vec![term_index("name")]);
    driver.submit(1, prepare(declaration));
    driver.sync();
    assert!(applier.rebuilding(), "the build runs");
    let mut txn = begin(&applier);
    write(&mut txn, vec![insert_literal("c", "name", "pear")]);
    driver.submit(2, prepare(txn));
    driver.sync();
    assert!(applier.rebuilding(), "committed during the build");
    applier.executor().run_until_idle();
    assert!(!applier.rebuilding(), "swapped in");
    w.replies = driver.replies.clone();
    w.indexed = indexed_term(&applier, "name", "pear");
    w.subject = uid_of(&applier, "c");
}

#[then("once the build is swapped in, the term index of `name` lists `c` under `pear`")]
fn in_the_built_index(w: &mut Steps) {
    assert!(
        w.replies.iter().all(|r| r.outcome().is_ok()),
        "{:?}",
        w.replies
    );
    assert_eq!(w.indexed, vec![w.subject]);
}

/// Registers a session through `pipe` under `ticket`: its number.
async fn register(pipe: &mut Pipe<'_>, ticket: u64) -> u64 {
    pipe.register(ticket, 0).expect("appended");
    pipe.sync(0).await.expect("synced");
    let reply = pipe.replies_mut().pop().expect("answered");
    assert_eq!(reply.ticket(), ticket);
    reply.outcome().clone().expect("registered")
}

#[when("a client commits request 7 of its session, loses the reply, and sends request 7 again")]
async fn retried(w: &mut Steps) {
    let applier = applier();
    setup(&applier, vec![insert("a", "knows", "b")]);
    let (_storage, log) = open_wal().await;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).expect("it opens");
    let session = register(&mut pipe, 100).await;
    for request in 1..=6u64 {
        let mut txn = begin(&applier);
        write(&mut txn, vec![insert(&format!("s{request}"), "knows", "b")]);
        pipe.commit(request, prepare_in(txn, session, request), 0)
            .expect("appended");
        pipe.sync(0).await.expect("synced");
    }
    pipe.replies_mut().clear();
    let seven = || {
        let mut txn = begin(&applier);
        write(&mut txn, vec![insert("x", "knows", "y")]);
        prepare_in(txn, session, 7)
    };
    pipe.commit(7, seven(), 0).expect("appended");
    pipe.sync(0).await.expect("synced");
    let first = pipe.replies_mut().pop().expect("the reply, lost");
    let applied = applier.quads().lists().applied_ts();
    pipe.commit(8, seven(), 0).expect("answered");
    pipe.sync(0).await.expect("synced");
    w.replies = vec![first];
    w.replies.append(pipe.replies_mut());
    assert_eq!(
        applier.quads().lists().applied_ts(),
        applied,
        "the retry applies nothing"
    );
    w.held = held(&applier, "x", "knows");
}

#[then("it receives the same reply and commit version, and the transaction's writes are held once")]
fn same_reply(w: &mut Steps) {
    assert_eq!(w.replies.len(), 2, "{:?}", w.replies);
    assert!(w.replies[0].outcome().is_ok());
    assert_eq!(w.replies[0].outcome(), w.replies[1].outcome());
    assert_eq!(w.held.len(), 1);
}

#[when("a session is evicted from the full table and its client sends its next request")]
async fn evicted(w: &mut Steps) {
    let applier = applier();
    setup(&applier, vec![insert("a", "knows", "b")]);
    let (_storage, log) = open_wal().await;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded())
        .expect("it opens")
        .with_sessions(1);
    let oldest = register(&mut pipe, 1).await;
    register(&mut pipe, 2).await;
    let mut txn = begin(&applier);
    write(&mut txn, vec![insert("e", "knows", "b")]);
    pipe.commit(3, prepare_in(txn, oldest, 1), 0)
        .expect("answered");
    pipe.sync(0).await.expect("synced");
    w.replies = std::mem::take(pipe.replies_mut());
    w.held = held(&applier, "e", "knows");
}

#[then("the request is refused with an error saying the session was evicted")]
fn told_evicted(w: &mut Steps) {
    assert_eq!(w.replies.len(), 1);
    let Err(error) = w.replies[0].outcome() else {
        panic!("committed: {:?}", w.replies);
    };
    assert!(
        matches!(error.0, CommitErrorEnum::SessionEvicted { .. }),
        "{error:?}"
    );
    assert!(error.to_string().contains("evicted"));
    assert!(w.held.is_empty());
}

#[when("the commit queue is full and a transaction commits")]
async fn queue_full(w: &mut Steps) {
    let applier = applier();
    setup(&applier, vec![insert("a", "knows", "b")]);
    let (_storage, log) = open_wal().await;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded())
        .expect("it opens")
        .with_queue(2);
    // One round in flight and two waiting fill the queue.
    for (ticket, subject) in [(1, "p"), (2, "q"), (3, "r"), (4, "late")] {
        let mut txn = begin(&applier);
        write(&mut txn, vec![insert(subject, "knows", "b")]);
        pipe.commit(ticket, prepare(txn), 0)
            .expect("taken or refused");
    }
    w.replies = std::mem::take(pipe.replies_mut());
    pipe.sync(0).await.expect("synced");
    pipe.sync(0).await.expect("synced");
    w.held = held(&applier, "late", "knows");
}

#[then("the commit fails with a retryable busy error, and nothing it wrote is held")]
fn busy(w: &mut Steps) {
    assert_eq!(w.replies, vec![Reply::new(4, Err(CommitError::Busy(2)))]);
    assert!(CommitError::Busy(2).is_retryable());
    assert!(w.held.is_empty());
}

#[when("a transaction's write set grows past 1 MiB and it commits")]
fn too_large(w: &mut Steps) {
    let applier = applier();
    setup(&applier, vec![insert("a", "knows", "b")]);
    let mut txn = begin(&applier);
    write(
        &mut txn,
        vec![insert_literal("big", "note", &"x".repeat(1_100_000))],
    );
    w.refusal = txn.commit(0, 0, &[], 0).err();
    w.held = held(&applier, "big", "note");
}

#[then(
    "the commit fails with a too-large error that is not marked retryable, and nothing it wrote is held"
)]
fn too_large_refused(w: &mut Steps) {
    let Some(error) = &w.refusal else {
        panic!("the transaction prepared");
    };
    assert!(
        matches!(error.0, TxnErrorEnum::WriteSetTooLarge { .. }),
        "{error:?}"
    );
    assert!(!error.is_retryable());
    assert!(w.held.is_empty());
}

#[when("a list with deltas at 10 and 12 is rolled up")]
fn rolled_up(w: &mut Steps) {
    let applier = applier();
    setup(&applier, vec![insert("a", "knows", "b")]);
    let mut driver = Driver::new(&applier, unbounded());
    // Clocks of 5 µs and 6 µs: versions 10 and 12, each a delta of `a`'s list of `knows`.
    for (ticket, (object, now_us)) in [("c", 5u64), ("d", 6)].into_iter().enumerate() {
        driver.now_us = now_us;
        let mut txn = begin(&applier);
        write(&mut txn, vec![insert("a", "knows", object)]);
        driver.submit(ticket as u64, prepare(txn));
        driver.sync();
    }
    let key = QuadKey::Forward {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: uid_of(&applier, "knows"),
        subject: uid_of(&applier, "a"),
    }
    .to_bytes();
    let lists = applier.quads().lists();
    let deltas: Vec<u64> = lists
        .store()
        .contents()
        .versions(&key)
        .iter()
        .map(|v| v.version())
        .collect();
    assert_eq!(&deltas[..2], &[12, 10], "the deltas, newest first");
    assert!(lists.rollup_key(&key).expect("rolled up"));
    w.versions = vec![lists.store().contents().versions(&key)[0].version()];
    driver.now_us = 0;
    let mut next = begin(&applier);
    write(&mut next, vec![insert("e", "knows", "b")]);
    driver.submit(9, prepare(next));
    driver.sync();
    w.replies = driver.replies.clone();
}

#[then("its complete version is written at 13, and the next commit's version is even")]
fn between_versions(w: &mut Steps) {
    assert_eq!(w.versions, vec![13]);
    let reply = w.replies.iter().find(|r| r.ticket() == 9).expect("decided");
    assert_eq!(reply.outcome(), &Ok(14));
}

// Requirement: Commit-time operations

/// The unique declaration of `epi.id`, committed as set-up.
fn unique_id(applier: &Applier) {
    let declared = PredicateDeclaration::new(ex("epi.id")).with_unique(true);
    setup(
        applier,
        vec![Change::Declare(Declaration::Predicate(declared))],
    );
}

/// The upsert-and-link of the node with `epi.id` `value`, `properties` for its creation, linking each of `memories`
/// to it by `HAS_ENTITY`.
fn upsert(value: &str, properties: Vec<(NamedNode, Term)>, memories: &[&str]) -> Operation {
    Operation::UpsertAndLink {
        graph: GraphName::DefaultGraph,
        predicate: ex("epi.id"),
        value: Literal::new_simple_literal(value).into(),
        properties,
        links: memories
            .iter()
            .map(|m| OperationLink::to_node(ex("HAS_ENTITY"), ex(m).into()))
            .collect(),
    }
}

/// Commits `txns` through a pipeline over a fresh log as concurrent transactions do: the first alone, half the rest
/// waiting while it syncs and decided over it before it is applied, then the others while that round syncs, every round
/// synced. The replies in ticket order.
async fn commit_concurrently(
    applier: &Applier,
    txns: Vec<ultragraphdb_transactions_txn::Prepared>,
) -> Vec<Reply> {
    let (_storage, log) = open_wal().await;
    let mut pipe: Pipe<'_> = Pipeline::new(applier, log, unbounded()).expect("it opens");
    let n = txns.len();
    for (ticket, prepared) in txns.into_iter().enumerate() {
        pipe.commit(ticket as u64, prepared, 0).expect("appended");
        // The first round stays in flight while half the rest wait: they are decided over it, unapplied.
        if ticket == n / 2 {
            pipe.sync(0).await.expect("synced");
        }
    }
    for _ in 0..3 {
        pipe.sync(0).await.expect("synced");
    }
    let mut replies = std::mem::take(pipe.replies_mut());
    replies.sort_by_key(Reply::ticket);
    replies
}

/// The statements of predicate `<name>` the store holds, by its statistics.
fn statements_of(applier: &Applier, name: &str) -> i64 {
    let Some(predicate) = applier
        .quads()
        .uid_of(NS, ex(name).as_str())
        .expect("the dictionary reads")
    else {
        return 0;
    };
    let watermark = applier.quads().lists().applied_ts();
    applier
        .quads()
        .statistics(NS, DEFAULT_GRAPH, predicate, watermark)
        .expect("the statistics read")
        .statements()
}

/// The encoded objects of the list `key` at the watermark.
fn objects_of(applier: &Applier, key: &QuadKey) -> Vec<EncodedTerm> {
    let watermark = applier.quads().lists().applied_ts();
    applier
        .quads()
        .objects(&key.to_bytes(), watermark)
        .expect("the list reads")
        .into_iter()
        .map(|(object, _)| object)
        .collect()
}

/// The forward list of `subject`'s `<predicate>`.
fn forward(applier: &Applier, subject: u64, predicate: &str) -> QuadKey {
    QuadKey::Forward {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: uid_of(applier, predicate),
        subject,
    }
}

/// The one UID every reply's single result names, and how many replies said they created it.
fn one_uid(replies: &[Reply]) -> (u64, usize) {
    let uids: Vec<u64> = replies
        .iter()
        .map(|r| {
            assert_eq!(r.results().len(), 1, "{r:?}");
            r.results()[0].uid()
        })
        .collect();
    assert!(uids.iter().all(|u| *u == uids[0] && *u != 0), "{uids:?}");
    let created = replies.iter().filter(|r| r.results()[0].created()).count();
    (uids[0], created)
}

#[when(
    "eight transactions concurrently insert-if-absent a node with `epi.id` `\"x\"`, declared unique"
)]
async fn insert_if_absent_eight(w: &mut Steps) {
    let applier = applier();
    unique_id(&applier);
    let txns: Vec<_> = (0..8)
        .map(|_| {
            let mut txn = begin(&applier);
            txn.operate(
                Operation::InsertIfAbsent {
                    graph: GraphName::DefaultGraph,
                    predicate: ex("epi.id"),
                    value: Literal::new_simple_literal("x").into(),
                    properties: Vec::new(),
                },
                0,
            )
            .expect("asked");
            prepare(txn)
        })
        .collect();
    w.replies = commit_concurrently(&applier, txns).await;
    w.statements = vec![statements_of(&applier, "epi.id")];
    let (node, _) = one_uid(&w.replies);
    w.terms = vec![
        applier
            .quads()
            .list_terms(
                NS,
                &forward(&applier, node, "epi.id").to_bytes(),
                applier.quads().lists().applied_ts(),
            )
            .expect("the node reads"),
    ];
}

#[then("all eight commit, all receive the same UID, and one node holds `epi.id` `\"x\"`")]
fn one_node(w: &mut Steps) {
    assert_eq!(w.replies.len(), 8);
    assert!(
        w.replies.iter().all(|r| r.outcome().is_ok()),
        "{:?}",
        w.replies
    );
    let (_, created) = one_uid(&w.replies);
    assert_eq!(created, 1, "one reply created the node");
    assert_eq!(w.statements, vec![1], "one statement of epi.id");
    assert_eq!(
        w.terms,
        vec![vec![Term::from(Literal::new_simple_literal("x"))]]
    );
}

#[when(
    "eight transactions each create a memory `m1` to `m8` and concurrently upsert-and-link the entity with `epi.id` `\"x\"`, declared unique, each linking its memory to it by `HAS_ENTITY`"
)]
async fn upsert_and_link_eight(w: &mut Steps) {
    let applier = applier();
    unique_id(&applier);
    // `kind` known before: eight first creations of one IRI conflict on its absence, the rule of new IRIs.
    setup(&applier, vec![insert_literal("m0", "kind", "memory")]);
    let memories: Vec<String> = (1..=8).map(|i| format!("m{i}")).collect();
    let txns: Vec<_> = memories
        .iter()
        .map(|memory| {
            let mut txn = begin(&applier);
            write(&mut txn, vec![insert_literal(memory, "kind", "memory")]);
            txn.operate(upsert("x", Vec::new(), &[memory]), 0)
                .expect("asked");
            prepare(txn)
        })
        .collect();
    w.replies = commit_concurrently(&applier, txns).await;
    w.statements = vec![statements_of(&applier, "epi.id")];
    let (node, _) = one_uid(&w.replies);
    w.subject = node;
    w.objects = memories
        .iter()
        .map(|m| {
            objects_of(
                &applier,
                &forward(&applier, uid_of(&applier, m), "HAS_ENTITY"),
            )
        })
        .collect();
    let reverse = QuadKey::Reverse {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: uid_of(&applier, "HAS_ENTITY"),
        object: EncodedTerm::uid(node),
    };
    let mut linked: Vec<u64> = objects_of(&applier, &reverse)
        .iter()
        .filter_map(EncodedTerm::as_uid)
        .collect();
    linked.sort_unstable();
    let mut expected: Vec<u64> = memories.iter().map(|m| uid_of(&applier, m)).collect();
    expected.sort_unstable();
    w.indexed = linked;
    w.versions = expected;
}

#[then(
    "all eight commit and receive the same UID, one node holds `epi.id` `\"x\"`, each memory holds `HAS_ENTITY` to that node, and the node's reverse list of `HAS_ENTITY` holds all eight memories"
)]
fn linked_once(w: &mut Steps) {
    assert_eq!(w.replies.len(), 8);
    assert!(
        w.replies.iter().all(|r| r.outcome().is_ok()),
        "{:?}",
        w.replies
    );
    let (node, created) = one_uid(&w.replies);
    assert_eq!(created, 1);
    assert_eq!(w.statements, vec![1]);
    for objects in &w.objects {
        assert_eq!(objects, &vec![EncodedTerm::uid(node)]);
    }
    assert_eq!(w.indexed.len(), 8);
    assert_eq!(
        w.indexed, w.versions,
        "the reverse list holds the eight memories"
    );
}

#[when(
    "two transactions concurrently upsert-and-link `epi.id` `\"y\"`, one carrying the label `\"first\"` for creation and the other `\"second\"`"
)]
async fn creation_properties(w: &mut Steps) {
    let applier = applier();
    unique_id(&applier);
    let txns: Vec<_> = ["first", "second"]
        .iter()
        .map(|label| {
            let mut txn = begin(&applier);
            let properties = vec![(ex("label"), Literal::new_simple_literal(*label).into())];
            txn.operate(upsert("y", properties, &[]), 0).expect("asked");
            prepare(txn)
        })
        .collect();
    w.replies = commit_concurrently(&applier, txns).await;
    let (node, _) = one_uid(&w.replies);
    w.terms = vec![
        applier
            .quads()
            .list_terms(
                NS,
                &forward(&applier, node, "label").to_bytes(),
                applier.quads().lists().applied_ts(),
            )
            .expect("the label reads"),
    ];
}

#[then(
    "the node holds the label of the transaction committed first and not the other, and only that transaction's reply says the node was created"
)]
fn created_once(w: &mut Steps) {
    assert!(
        w.replies.iter().all(|r| r.outcome().is_ok()),
        "{:?}",
        w.replies
    );
    let first = w
        .replies
        .iter()
        .min_by_key(|r| r.outcome().clone().unwrap_or(u64::MAX))
        .expect("two");
    assert_eq!(first.ticket(), 0, "the first ticket committed first");
    assert!(w.replies[0].results()[0].created());
    assert!(!w.replies[1].results()[0].created());
    assert_eq!(
        w.terms,
        vec![vec![Term::from(Literal::new_simple_literal("first"))]]
    );
}

#[when(
    "a transaction upsert-and-links `epi.id` `\"z\"` and aborts with a conflict on another of its reads"
)]
async fn aborted_upsert(w: &mut Steps) {
    let applier = applier();
    unique_id(&applier);
    setup(&applier, vec![insert("a", "knows", "b")]);
    let mut txn = begin(&applier);
    read(&mut txn, "a", "knows");
    write(&mut txn, vec![insert_literal("mz", "kind", "memory")]);
    txn.operate(upsert("z", Vec::new(), &["mz"]), 0)
        .expect("asked");
    let mut other = begin(&applier);
    write(&mut other, vec![insert("a", "knows", "c")]);
    w.replies = commit_concurrently(&applier, vec![prepare(other), prepare(txn)]).await;
    w.statements = vec![
        statements_of(&applier, "epi.id"),
        statements_of(&applier, "HAS_ENTITY"),
    ];
}

#[then("no node holds `epi.id` `\"z\"` and no link of the transaction is held")]
fn nothing_upserted(w: &mut Steps) {
    assert!(w.replies[0].outcome().is_ok(), "{:?}", w.replies);
    assert_eq!(
        w.replies[1].outcome(),
        &Err(CommitError::Conflict(ConflictKind::List))
    );
    assert!(w.replies[1].results().is_empty());
    assert_eq!(w.statements, vec![0, 0]);
}

#[when(
    "a transaction inserts `<a> <seen> true` and compare-and-sets `status` of `a` from `\"open\"` to `\"closed\"` while `status` is `\"done\"`"
)]
async fn failed_cas(w: &mut Steps) {
    let applier = applier();
    setup(&applier, vec![insert_literal("a", "status", "done")]);
    let mut txn = begin(&applier);
    write(
        &mut txn,
        vec![Change::Insert {
            graph: GraphName::DefaultGraph,
            subject: ex("a").into(),
            predicate: ex("seen"),
            object: Literal::from(true).into(),
        }],
    );
    txn.operate(
        Operation::CompareAndSet {
            graph: GraphName::DefaultGraph,
            subject: ex("a").into(),
            predicate: ex("status"),
            expected: Literal::new_simple_literal("open").into(),
            new: Literal::new_simple_literal("closed").into(),
        },
        0,
    )
    .expect("asked");
    w.replies = commit_concurrently(&applier, vec![prepare(txn)]).await;
    w.terms = vec![held(&applier, "a", "status"), held(&applier, "a", "seen")];
}

#[then(
    "the transaction is refused reporting `\"done\"`, `status` of `a` stays `\"done\"`, and `<a> <seen> true` is not held"
)]
fn cas_refused(w: &mut Steps) {
    let Err(error) = w.replies[0].outcome() else {
        panic!("committed: {:?}", w.replies);
    };
    assert_eq!(
        error,
        &CommitError::OperationFailed(Some("\"done\"".to_string()))
    );
    assert_eq!(error.found(), Some("\"done\""));
    assert!(!error.is_retryable());
    assert_eq!(
        w.terms,
        vec![
            vec![Term::from(Literal::new_simple_literal("done"))],
            vec![]
        ]
    );
}

#[when("a hundred transactions concurrently add 1 to the counter `next-id`")]
async fn counter_adds(w: &mut Steps) {
    let applier = applier();
    let start = Change::Insert {
        graph: GraphName::DefaultGraph,
        subject: ex("counters").into(),
        predicate: ex("next-id"),
        object: Literal::from(5i64).into(),
    };
    setup(&applier, vec![start]);
    let txns: Vec<_> = (0..100)
        .map(|_| {
            let mut txn = begin(&applier);
            txn.operate(
                Operation::CounterAdd {
                    graph: GraphName::DefaultGraph,
                    subject: ex("counters").into(),
                    predicate: ex("next-id"),
                    delta: 1,
                },
                0,
            )
            .expect("asked");
            prepare(txn)
        })
        .collect();
    w.replies = commit_concurrently(&applier, txns).await;
    w.terms = vec![held(&applier, "counters", "next-id")];
}

#[then("all commit and `next-id` has grown by 100")]
fn grown_by_100(w: &mut Steps) {
    assert_eq!(w.replies.len(), 100);
    assert!(
        w.replies.iter().all(|r| r.outcome().is_ok()),
        "{:?}",
        w.replies
    );
    // 5 at the start, plus 100 adds of 1.
    assert_eq!(w.terms, vec![vec![Term::from(Literal::from(105i64))]]);
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()),
    );
}
