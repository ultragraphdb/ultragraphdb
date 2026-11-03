/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The commit-rounds, sequencer, transactions, write-ahead-log and drops scenarios the commit step carries: real
//! transactions on a changeset applier over the simulated store, decided by the commit step, each synced round applied
//! by the driver standing in for the pipeline, or by the pipeline itself over the write-ahead log on simulated storage.
//! The test delivers the sequencer's closes as inputs; a second partition is a second commit step over a store of its
//! own. Scenarios about epochs alone run the commit step over hand-built proposals. Every version is written from the
//! layout `epoch << 22 | sequence` ([`version`]): a step after a store's applied version of epoch 0 stamps epoch 1
//! from sequence 0.

use cucumber::{World, then, when};
use oxrdf::{GraphName, Literal, NamedNode, Term};
use ultragraphdb_cluster_placement::{NodeId, PartitionId, PlacementMap};
use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_ids::{
    Bucket, CanonicalKey, Epoch, Namespace, SessionToken, SystemId, TypePrefix,
};
use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey};
use ultragraphdb_core_mapping::{Change, Declaration, PredicateDeclaration};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::{ConflictKind, ErrorCode, OperationResult, RequestTerm};
use ultragraphdb_store_apply::{Applied, DropRequest, Operation, OperationLink};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_store_sim::{SimListCache, SimStore, SteppedExecutor};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_traits_blocks::TestBlockSource;
use ultragraphdb_traits_log::{OrderedLog, Position};
use ultragraphdb_traits_store::Store;
use ultragraphdb_transactions_commit::{
    ApplierOperator, CloseRequest, CommitError, CommitErrorEnum, CommitRequest, CommitStep, Input,
    Pipeline, PipelineError, RecoveryError, RecoveryErrorEnum, Replay, Reply, RoundBounds,
    RoundRecord, Sessions, StepOutput, StoredReply,
};
use ultragraphdb_transactions_log::{FaultMode, SimStorage};
use ultragraphdb_transactions_txn::{MAX_WRITE_SET, Prepared, TxnBuffers, TxnError, TxnErrorEnum};

/// The unit tests' fixture and doubles, shared by path.
pub mod utils {
    #[path = "fixture.rs"]
    pub mod fixture;
    #[path = "proposal_double.rs"]
    pub mod proposal_double;
}

use crate::utils::fixture::{
    Applier, Driver, Log, NS, Refused, applier, applier_over, begin, changeset, delete_literal, ex,
    held, id_in, id_of, insert, insert_literal, open_wal, prepare, prepare_in, read, reopen, setup,
    sim_store, version, write,
};
use crate::utils::proposal_double::{Reads, TestProposal, Writes, proposal, registration};

#[derive(Debug, Default, World)]
struct Steps {
    appended: Vec<RoundRecord>,
    syncs_before_reply: Vec<usize>,
    replies: Vec<Reply>,
    held: Vec<Term>,
    decisions: Vec<Vec<Reply>>,
    entries: Vec<PredicateDeclaration>,
    /// System IDs read back, and the ones expected, where the store names them.
    ids: Vec<SystemId>,
    expected_ids: Vec<SystemId>,
    retry_refused: bool,
    refusal: Option<TxnError>,
    versions: Vec<u64>,
    /// The statements of a predicate held, by the predicate's statistics.
    statements: Vec<i64>,
    /// Encoded objects read back, by list.
    objects: Vec<Vec<EncodedTerm>>,
    terms: Vec<Vec<Term>>,
    /// Each case's check, of a scenario run over several.
    checks: Vec<bool>,
    recovery: Option<RecoveryError>,
    version: u64,
    /// Epochs observed: delivered or closed, as each scenario names.
    epochs: Vec<u64>,
    close_requests: Vec<Epoch>,
    /// The commits a step held, observed after each input.
    held_counts: Vec<usize>,
    tokens: Vec<SessionToken>,
    stored: Vec<StoredReply>,
    errors: Vec<CommitError>,
    /// A session's last committed request and its version in a partition's table, observed after each input.
    sessions: Vec<Option<(u64, u64)>>,
    /// IRIs a reply named.
    names: Vec<String>,
}

/// 2^22 − 2, the last sequence of an epoch, by hand: 4,194,304 − 2.
const LAST: u64 = 4_194_302;
/// The low 22 bits of a version, its sequence, by hand: 2^22 − 1 = 4,194,303.
const SEQUENCE_MASK: u64 = 4_194_303;
/// The session the session scenarios commit through: its identity uses the high bits of the 128.
const SESSION: u128 = 0xAB << 100;

type Step = CommitStep<TestProposal>;
type Out = StepOutput<TestProposal>;
type Pipe<'a> =
    Pipeline<'a, Log, SimStore, SimListCache<CachedList>, SteppedExecutor, TestBlockSource>;

fn unbounded() -> RoundBounds {
    RoundBounds::new(u32::MAX, u32::MAX)
}

fn epoch(n: u64) -> TestResult<Epoch> {
    Epoch::new(n).context("the epoch")
}

/// A step over hand-built proposals after version `previous`, its latest delivered close `delivered`.
fn step_at(previous: u64, delivered: u64) -> TestResult<Step> {
    Ok(CommitStep::new(unbounded(), previous).with_delivered(epoch(delivered)?))
}

/// A proposal writing the point `k{n}`, reading nothing.
// Read at a version above every version the scenarios stamp, so no step's history floor makes it too old.
fn writer(n: u64) -> TestResult<TestProposal> {
    proposal(
        u64::MAX >> 1,
        &Reads::default(),
        &Writes::point(format!("k{n}").as_bytes()),
    )
}

/// Steps `input` at `now_us`, returning the output.
fn run(step: &mut Step, input: Input<TestProposal>, now_us: u64) -> Out {
    let mut out = StepOutput::new();
    step.step(input, now_us, &mut out);
    out
}

fn commit(step: &mut Step, ticket: u64, p: TestProposal, now_us: u64) -> Out {
    run(step, Input::Commit(CommitRequest::new(ticket, p)), now_us)
}

fn deliver(step: &mut Step, delivered: u64) -> TestResult<Out> {
    Ok(run(
        step,
        Input::Close(crate::utils::fixture::close(
            delivered,
            Vec::new(),
            Vec::new(),
        )?),
        0,
    ))
}

/// Each reply's ticket and outcome, its token left out.
fn outcomes(replies: &[Reply]) -> Vec<(u64, Result<u64, CommitError>)> {
    replies
        .iter()
        .map(|r| (r.ticket(), r.outcome().clone()))
        .collect()
}

/// The commit versions of `replies`, every one committed.
fn committed(replies: &[Reply]) -> TestResult<Vec<u64>> {
    replies
        .iter()
        .map(|r| r.outcome().clone().context("committed"))
        .collect()
}

/// A driver over `applier` whose commit step decides for `partition`, its latest delivered close `delivered`.
fn driver_at(applier: &Applier, partition: u32, delivered: u64) -> TestResult<Driver<'_>> {
    let mut driver = Driver::new(applier, unbounded());
    driver.step = CommitStep::new(unbounded(), applier.quads().lists().applied_ts())
        .with_delivered(epoch(delivered)?)
        .with_partition(partition);
    Ok(driver)
}

/// Commits `<subject> <knows> <b>` through `driver` as request `request` of `session`, under the request's number as
/// its ticket, and syncs its round.
fn commit_in(
    driver: &mut Driver<'_>,
    applier: &Applier,
    session: u128,
    request: u64,
    subject: &str,
) -> TestResult {
    let mut txn = begin(applier)?;
    write(&mut txn, vec![insert(subject, "knows", "b")])?;
    driver.submit(request, prepare_in(txn, session, request)?)?;
    driver.sync()
}

#[when("one transaction commits on an idle node")]
fn lone_commit(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    w.version = setup(&applier, vec![insert("a", "knows", "b")])?;
    let mut driver = Driver::new(&applier, unbounded());
    let mut txn = begin(&applier)?;
    write(&mut txn, vec![insert("c", "knows", "d")])?;
    driver.submit(1, prepare(txn)?)?;
    assert!(driver.reply(1).is_none(), "no reply before the sync");
    driver.sync()?;
    w.appended = driver.appended.clone();
    w.syncs_before_reply = vec![driver.syncs];
    w.replies = driver.replies.clone();
    w.held = held(&applier, "c", "knows")?;
    Ok(())
}

#[then("it forms a round of its own and is acknowledged after one log sync and its apply")]
fn alone_after_one_sync(w: &mut Steps) {
    assert_eq!(w.appended.len(), 1);
    assert_eq!(w.appended[0].transactions(), 1);
    assert_eq!(w.syncs_before_reply, vec![1]);
    // The first commit after the set-up, of epoch 0: epoch 1, sequence 0.
    assert_eq!(outcomes(&w.replies), vec![(1, Ok(version(1, 0)))]);
    // Applied: a transaction beginning now reads it.
    assert_eq!(w.held.len(), 1);
}

#[when("ten transactions commit while a round's sync is in progress")]
fn ten_during_a_sync(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    w.version = setup(&applier, vec![insert("a", "knows", "b")])?;
    let mut driver = Driver::new(&applier, unbounded());
    let mut first = begin(&applier)?;
    write(&mut first, vec![insert("first", "knows", "b")])?;
    driver.submit(0, prepare(first)?)?;
    let txns: Vec<_> = (1..=10)
        .map(|i| {
            let mut txn = begin(&applier)?;
            write(&mut txn, vec![insert(&format!("s{i}"), "knows", "b")])?;
            prepare(txn)
        })
        .collect::<TestResult<Vec<_>>>()?;
    for (i, prepared) in txns.into_iter().enumerate() {
        driver.submit(i as u64 + 1, prepared)?;
    }
    assert_eq!(
        driver.appended.len(),
        1,
        "the ten wait for the sync in progress"
    );
    driver.sync()?;
    driver.sync()?;
    w.appended = driver.appended.clone();
    w.syncs_before_reply = vec![driver.syncs];
    w.replies = driver.replies.clone();
    Ok(())
}

#[then("they are decided in the next round, written with one log append and one sync")]
fn ten_share_a_round(w: &mut Steps) -> TestResult {
    assert_eq!(w.appended.len(), 2);
    assert_eq!(w.appended[1].transactions(), 10);
    assert_eq!(w.appended[1].committed(), 10);
    assert_eq!(w.syncs_before_reply, vec![2]);
    // The first alone at (1, 0); the ten in arrival order at sequences 2 to 20 of epoch 1, two apart, in one round.
    let expected: Vec<(u64, Result<u64, CommitError>)> =
        (0..=10u64).map(|t| (t, Ok(version(1, 2 * t)))).collect();
    assert_eq!(outcomes(&w.replies), expected);
    Ok(())
}

// Requirement: Commit versions are built from the epoch.

#[when(
    "epoch 9 is the latest close delivered to a partition and it admits its first transaction since"
)]
fn first_after_close_9(w: &mut Steps) -> TestResult {
    let mut step = step_at(0, 9)?;
    let _round = commit(&mut step, 1, writer(1)?, 0);
    w.versions = committed(run(&mut step, Input::Synced, 0).replies())?;
    Ok(())
}

#[then("the version's upper 42 bits are 10 and its lower 22 bits are 0")]
fn epoch_10_sequence_0(w: &mut Steps) {
    assert_eq!(w.versions.len(), 1);
    assert_eq!(w.versions[0] >> 22, 10);
    assert_eq!(w.versions[0] & SEQUENCE_MASK, 0);
}

#[when("a partition admits three transactions in epoch 10")]
fn three_in_epoch_10(w: &mut Steps) -> TestResult {
    let mut step = step_at(0, 9)?;
    for ticket in 1..=3 {
        let _out = commit(&mut step, ticket, writer(ticket)?, 0);
    }
    w.versions = committed(run(&mut step, Input::Synced, 0).replies())?;
    w.versions
        .extend(committed(run(&mut step, Input::Synced, 0).replies())?);
    Ok(())
}

#[then("their sequences are 0, 2 and 4")]
fn sequences_0_2_4(w: &mut Steps) {
    let epochs: Vec<u64> = w.versions.iter().map(|v| v >> 22).collect();
    let sequences: Vec<u64> = w.versions.iter().map(|v| v & SEQUENCE_MASK).collect();
    assert_eq!(epochs, vec![10, 10, 10]);
    assert_eq!(sequences, vec![0, 2, 4]);
}

#[when("a partition's sequence in epoch 10 reaches 2^22 − 2 and it admits another transaction")]
fn epoch_fills(w: &mut Steps) -> TestResult {
    let mut step = step_at(version(10, LAST - 2), 9)?;
    // The last sequence is taken, by a commit of its own.
    let mut out = commit(&mut step, 1, writer(1)?, 0);
    w.versions
        .extend(out.append().map(|(r, _)| r.first_version()));
    let _synced = run(&mut step, Input::Synced, 0);
    // The next finds the epoch full.
    out = commit(&mut step, 2, writer(2)?, 7);
    w.close_requests.extend(out.close_request());
    w.held_counts.push(step.held());
    w.checks
        .push(out.append().is_none() && out.replies().is_empty());
    out = deliver(&mut step, 10)?;
    w.held_counts.push(step.held());
    w.appended.extend(out.append().map(|(r, _)| r));
    w.replies = run(&mut step, Input::Synced, 0).replies().to_vec();
    Ok(())
}

#[then(
    "it asks the sequencer to close epoch 10, holds the transaction until the close of 10 is delivered, and stamps it with epoch 11 and sequence 0"
)]
fn close_asked_and_waited(w: &mut Steps) -> TestResult {
    assert_eq!(w.versions, vec![version(10, LAST)], "the last sequence");
    assert_eq!(w.close_requests, vec![epoch(10)?]);
    assert_eq!(w.held_counts, vec![1, 0], "held until the close of 10");
    assert_eq!(w.checks, vec![true], "nothing logged or answered meanwhile");
    assert_eq!(w.appended.len(), 1);
    assert_eq!(w.appended[0].first_version(), version(11, 0));
    assert_eq!(outcomes(&w.replies), vec![(2, Ok(version(11, 0)))]);
    Ok(())
}

#[when(
    "a partition's sequence in epoch 10 is full, `close_request_ms` is 500, and its close request is still unanswered 500 ms after it was sent"
)]
fn close_unanswered(w: &mut Steps) -> TestResult {
    // `epoch_ms_max` 250: 500 is at least twice it, as the setting requires.
    let mut step = CommitStep::new(unbounded(), version(10, LAST))
        .with_delivered(epoch(9)?)
        .with_close_request(CloseRequest::new(500, 250).context("the setting")?);
    let asked = 1_000;
    let mut out = commit(&mut step, 1, writer(1)?, asked);
    w.close_requests.extend(out.close_request());
    // 499,999 µs after the request the sequencer still counts as reachable.
    out = run(&mut step, Input::Tick, asked + 499_999);
    w.checks.push(out.replies().is_empty());
    // 500 ms after it, the held commit is refused; a commit arriving meanwhile is refused at once.
    for refused in [
        run(&mut step, Input::Tick, asked + 500_000),
        commit(&mut step, 2, writer(2)?, asked + 600_000),
    ] {
        w.replies.extend(refused.replies().iter().cloned());
        w.checks
            .push(refused.append().is_none() && refused.released().len() == 1);
    }
    w.held_counts.push(step.held());
    let _delivered = deliver(&mut step, 10)?;
    out = commit(&mut step, 3, writer(3)?, asked + 700_000);
    w.versions
        .extend(out.append().map(|(r, _)| r.first_version()));
    Ok(())
}

#[then(
    "every transaction it holds or admits is refused as busy, marked retryable, writing nothing, until the close of 10 is delivered"
)]
fn busy_until_the_close(w: &mut Steps) -> TestResult {
    assert_eq!(w.close_requests, vec![epoch(10)?]);
    // Still held just before; then nothing appended and each proposal released, its writes gone.
    assert_eq!(w.checks, vec![true, true, true]);
    assert_eq!(w.held_counts, vec![0]);
    let tickets: Vec<u64> = w.replies.iter().map(Reply::ticket).collect();
    assert_eq!(tickets, vec![1, 2]);
    for reply in &w.replies {
        let error = reply.outcome().clone().refused("refused busy")?;
        assert_eq!(error.code(), ErrorCode::Busy);
        assert!(error.is_retryable());
    }
    // Once the close of 10 is delivered, commits flow again in epoch 11.
    assert_eq!(w.versions, vec![version(11, 0)]);
    Ok(())
}

// Requirement: Validation is exact.

#[when("in one round T1 aborts, and T2's reads conflict only with what T1 would have written")]
fn aborted_write(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    w.version = setup(
        &applier,
        vec![
            insert_literal("x", "name", "X"),
            insert_literal("y", "name", "Y"),
            insert_literal("z", "name", "Z"),
            insert("a", "knows", "b"),
        ],
    )?;
    let mut driver = Driver::new(&applier, unbounded());
    // T0 writes x; T1 read x and writes y, so it aborts; T2 read y and writes z.
    let mut t0 = begin(&applier)?;
    let mut t1 = begin(&applier)?;
    let mut t2 = begin(&applier)?;
    write(
        &mut t0,
        vec![
            delete_literal("x", "name", "X"),
            insert_literal("x", "name", "X0"),
        ],
    )?;
    let _read: Vec<Term> = read(&mut t1, "x", "name")?;
    write(
        &mut t1,
        vec![
            delete_literal("y", "name", "Y"),
            insert_literal("y", "name", "Y1"),
        ],
    )?;
    let _read_y: Vec<Term> = read(&mut t2, "y", "name")?;
    write(
        &mut t2,
        vec![
            delete_literal("z", "name", "Z"),
            insert_literal("z", "name", "Z2"),
        ],
    )?;
    let mut blocker = begin(&applier)?;
    write(&mut blocker, vec![insert("blocker", "knows", "b")])?;
    driver.submit(9, prepare(blocker)?)?;
    driver.submit(0, prepare(t0)?)?;
    driver.submit(1, prepare(t1)?)?;
    driver.submit(2, prepare(t2)?)?;
    driver.sync()?;
    driver.sync()?;
    assert_eq!(driver.appended.len(), 2);
    assert_eq!(
        driver.appended[1].transactions(),
        3,
        "the three share one round"
    );
    w.replies = driver.replies.clone();
    w.held = held(&applier, "z", "name")?;
    Ok(())
}

#[then("T2 commits")]
fn t2_commits(w: &mut Steps) -> TestResult {
    // The blocker alone at (1, 0); then T0 at (1, 2), T1 aborted taking none, T2 at (1, 4).
    assert_eq!(
        outcomes(&w.replies),
        vec![
            (9, Ok(version(1, 0))),
            (0, Ok(version(1, 2))),
            (1, Err(CommitError::conflict(ConflictKind::List))),
            (2, Ok(version(1, 4)))
        ]
    );
    assert_eq!(w.held, vec![Term::from(Literal::new_simple_literal("Z2"))]);
    Ok(())
}

/// A round of twelve transactions over five keys, from the same state each time.
fn decide_round() -> TestResult<Vec<Reply>> {
    let mut step = CommitStep::<TestProposal>::new(unbounded(), 0);
    let mut replies = Vec::new();
    let first = proposal(0, &Reads::default(), &Writes::point(b"k0"))?;
    replies.extend(commit(&mut step, 0, first, 0).replies().iter().cloned());
    for t in 1..=12u64 {
        let read = [b'k', b'0' + (t % 5) as u8];
        let written = [b'k', b'0' + (t % 4) as u8];
        let p = proposal(0, &Reads::list(&read), &Writes::point(&written))?;
        replies.extend(commit(&mut step, t, p, 0).replies().iter().cloned());
    }
    for _ in 0..2 {
        replies.extend(run(&mut step, Input::Synced, 0).replies().iter().cloned());
    }
    Ok(replies)
}

#[when("the same round is validated twice from the same state")]
fn twice(w: &mut Steps) -> TestResult {
    w.decisions = vec![decide_round()?, decide_round()?];
    Ok(())
}

#[then("both decide the same transactions committed and aborted")]
fn alike(w: &mut Steps) -> TestResult {
    assert_eq!(w.decisions[0].len(), 13);
    assert_eq!(w.decisions[0], w.decisions[1]);
    // And both decide what the round's serial order decides, worked by hand: 0 wrote k0 at (1, 0); t reads k(t % 5) and
    // writes k(t % 4) at read version 0, aborting once k(t % 5) was written before it. 1 to 4 read k1 to k4, free, and
    // write k1, k2, k3, k0; then only 9, reading k4, is free. The five admitted take sequences 2 to 10 of epoch 1.
    let admitted = [(1u64, 2u64), (2, 4), (3, 6), (4, 8), (9, 10)];
    let mut expected = vec![(0, Ok(version(1, 0)))];
    for t in 1..=12u64 {
        expected.push(match admitted.iter().find(|(a, _)| *a == t) {
            Some((_, sequence)) => (t, Ok(version(1, *sequence))),
            None => (t, Err(CommitError::conflict(ConflictKind::List))),
        });
    }
    assert_eq!(outcomes(&w.decisions[0]), expected);
    Ok(())
}

// Requirement: The conflict history is bounded by the oldest open read.

#[when(
    "the history has been pruned above a transaction's read version and the transaction commits"
)]
fn pruned_above(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    w.version = setup(&applier, vec![insert("a", "knows", "b")])?;
    let mut driver = Driver::new(&applier, unbounded());
    let mut old = begin(&applier)?;
    write(&mut old, vec![insert("old", "knows", "b")])?;
    // Two commits move the watermark past the old transaction's read version, and the oldest open read follows.
    for (ticket, subject) in [(1, "p"), (2, "q")] {
        let mut txn = begin(&applier)?;
        write(&mut txn, vec![insert(subject, "knows", "b")])?;
        driver.submit(ticket, prepare(txn)?)?;
        driver.sync()?;
    }
    let watermark = driver.step.watermark();
    driver
        .step
        .step(Input::OldestRead(watermark), 0, &mut driver.out);
    driver.submit(3, prepare(old)?)?;
    driver.sync()?;
    w.replies = driver.replies.clone();
    w.held = held(&applier, "old", "knows")?;
    Ok(())
}

#[then("the commit fails with a retryable too-old error")]
fn too_old(w: &mut Steps) -> TestResult {
    let reply = w
        .replies
        .iter()
        .find(|r| r.ticket() == 3)
        .context("decided")?;
    let error = reply.outcome().clone().refused("the old transaction")?;
    // It read at the set-up's version; two commits, at (1, 0) and (1, 2), moved the watermark, and the floor with it.
    assert_eq!(error, CommitError::too_old(w.version, version(1, 2)));
    assert!(error.is_retryable());
    assert!(w.held.is_empty());
    Ok(())
}

// Requirement: Acknowledgement at the visibility watermark.

#[when(
    "a transaction is acknowledged in a partition and a read-write transaction bound to that partition begins at any time after"
)]
async fn acknowledged_then_begins(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    w.version = setup(&applier, vec![insert("a", "knows", "b")])?;
    let (_storage, log) = open_wal().await?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).context("it opens")?;
    let mut first = begin(&applier)?;
    write(&mut first, vec![insert("c", "knows", "d")])?;
    pipe.commit(1, prepare(first)?, 0).context("appended")?;
    // Later commits keep rounds in flight: the second transaction begins right after the acknowledgement, while the
    // next round still syncs.
    let mut later = begin(&applier)?;
    write(&mut later, vec![insert("e", "knows", "d")])?;
    pipe.commit(2, prepare(later)?, 0).context("waits")?;
    pipe.sync(0).await.context("synced")?;
    w.replies = std::mem::take(pipe.replies_mut());
    assert!(pipe.needs_sync(), "the next round syncs");
    // One partition owning every bucket: the store is its own.
    let placement = PlacementMap::even(1, NodeId::new(1)).context("the placement")?;
    let mut second = begin(&applier)?.bind(partition(1)?, &placement);
    w.held = read(&mut second, "c", "knows")?;
    Ok(())
}

#[then("the second transaction reads the first one's writes")]
fn reads_the_first(w: &mut Steps) -> TestResult {
    assert_eq!(outcomes(&w.replies), vec![(1, Ok(version(1, 0)))]);
    assert_eq!(w.held, vec![Term::from(ex("d"))]);
    Ok(())
}

#[when("the log sync of a round fails")]
async fn sync_fails(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert("a", "knows", "b")])?;
    let (storage, log) = open_wal().await?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).context("it opens")?;
    for (ticket, subject) in [(1, "p"), (2, "q")] {
        let mut txn = begin(&applier)?;
        write(&mut txn, vec![insert(subject, "knows", "b")])?;
        pipe.commit(ticket, prepare(txn)?, 0)
            .context("appended or waiting")?;
    }
    storage.fail_at(storage.trace().len(), FaultMode::Before);
    assert!(pipe.sync(0).await.is_err());
    let mut later = begin(&applier)?;
    write(&mut later, vec![insert("r", "knows", "b")])?;
    pipe.commit(3, prepare(later)?, 0).context("answered")?;
    w.replies = std::mem::take(pipe.replies_mut());
    w.held = ["p", "q", "r"]
        .iter()
        .map(|s| held(&applier, s, "knows"))
        .collect::<TestResult<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
    Ok(())
}

#[then(
    "none of the round's transactions is acknowledged, and later commits of its partition are refused until restart"
)]
fn none_acknowledged(w: &mut Steps) -> TestResult {
    let unavailable: Vec<(u64, Result<u64, CommitError>)> = (1..=3)
        .map(|t| (t, Err(CommitError::unavailable())))
        .collect();
    assert_eq!(outcomes(&w.replies), unavailable);
    assert!(w.held.is_empty());
    Ok(())
}

// Requirement: Catalog changes, drops and index builds are ordered in rounds.

#[when("a round is gathering transactions of epoch 10 and the close of 10 is delivered")]
fn close_during_a_round(w: &mut Steps) -> TestResult {
    let mut step = step_at(0, 9)?;
    let mut out = commit(&mut step, 1, writer(1)?, 0);
    w.appended.extend(out.append().map(|(r, _)| r));
    out = deliver(&mut step, 10)?;
    w.checks.push(out.append().is_none());
    let _waiting = commit(&mut step, 2, writer(2)?, 0);
    for _ in 0..2 {
        out = run(&mut step, Input::Synced, 0);
        w.appended.extend(out.append().map(|(r, _)| r));
        w.replies.extend(out.replies().iter().cloned());
    }
    Ok(())
}

#[then("the round closes, and the next transaction is stamped with epoch 11 in a new round")]
fn next_round_in_epoch_11(w: &mut Steps) {
    // The round of epoch 10 is the one in flight when the close arrives, and the close opens no round beside it.
    assert_eq!(w.checks, vec![true]);
    let rounds: Vec<(u64, u32)> = w
        .appended
        .iter()
        .map(|r| (r.first_version(), r.transactions()))
        .collect();
    assert_eq!(rounds, vec![(version(10, 0), 1), (version(11, 0), 1)]);
    assert_eq!(
        outcomes(&w.replies),
        vec![(1, Ok(version(10, 0))), (2, Ok(version(11, 0)))]
    );
}

#[when(
    "two transactions of `ns` read before either commits, the first writes `<a> <score> 12` and the second `<b> <score> \"high\"`, and `score` was not declared"
)]
fn first_writes(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    // `score` is a known IRI, never written as a predicate, so the catalog of `ns` holds no entry for it.
    w.version = setup(&applier, vec![insert("z", "seen", "score")])?;
    let score = id_of(&applier, "score")?;
    assert!(applier.catalog(NS)?.predicate(score).is_none());
    let mut driver = Driver::new(&applier, unbounded());
    let mut first = begin(&applier)?;
    let mut second = begin(&applier)?;
    write(
        &mut first,
        vec![Change::Insert {
            graph: GraphName::DefaultGraph,
            subject: ex("a").into(),
            predicate: ex("score"),
            object: Literal::from(12i64).into(),
        }],
    )?;
    write(&mut second, vec![insert_literal("b", "score", "high")])?;
    driver.submit(1, prepare(first)?)?;
    driver.submit(2, prepare(second)?)?;
    driver.sync()?;
    driver.sync()?;
    w.replies = driver.replies.clone();
    let catalog = applier.catalog(NS)?;
    let snapshot = applier
        .quads()
        .store()
        .snapshot(applier.quads().lists().applied_ts())?;
    let stored = Catalog::load(&snapshot, NS)?;
    assert_eq!(*catalog, stored, "the catalog installed is the one stored");
    w.entries = stored
        .predicates()
        .filter(|(id, _)| *id == score)
        .map(|(_, d)| d.clone())
        .collect();
    Ok(())
}

#[then("both commit, and the catalog of `ns` holds `score` with the default entry once")]
fn both_first_writes_commit(w: &mut Steps) -> TestResult {
    // Neither aborts: the first alone at (1, 0), the second in the next round at (1, 2).
    assert_eq!(
        outcomes(&w.replies),
        vec![(1, Ok(version(1, 0))), (2, Ok(version(1, 2)))]
    );
    assert_eq!(w.entries, vec![PredicateDeclaration::new(ex("score"))]);
    Ok(())
}

/// The declaration of `<p>` with the `term` tokenizer, which starts a build when `<p>` holds data.
fn term_index(p: &str) -> Change {
    Change::Declare(Declaration::Predicate(
        PredicateDeclaration::new(ex(p)).with_tokenizers(vec!["term".to_string()]),
    ))
}

/// The subjects the `term` index of `<predicate>` lists under `word`, at the watermark.
fn indexed_term(applier: &Applier, predicate: &str, word: &str) -> TestResult<Vec<SystemId>> {
    let key = QuadKey::ValueIndex {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: id_of(applier, predicate)?,
        // Dgraph's identifier of `term` (`tok/tok.go`).
        tokenizer: 0x01,
        token: word.as_bytes().to_vec(),
    }
    .to_bytes();
    Ok(applier
        .quads()
        .objects(&key, applier.quads().lists().applied_ts())
        .context("the index reads")?
        .iter()
        .filter_map(|(o, _)| o.as_id())
        .collect())
}

/// The reply to `ticket` among `replies`.
fn reply_to(replies: &[Reply], ticket: u64) -> TestResult<&Reply> {
    replies
        .iter()
        .find(|r| r.ticket() == ticket)
        .context("decided")
}

#[when(
    "a round admits a declaration of `ns` that starts an index build of `p`, and a later transaction of the same round, executed against the catalog before the declaration, writes `p` in `ns`"
)]
fn build_then_write(w: &mut Steps) -> TestResult {
    // The catalog is read through its version (commit-rounds, stage 5.5): the declaration commits as version 2 of
    // `ns`, which the next close announces, so the round after it is decided against version 2. The later transaction
    // executed against version 1, the catalog before the declaration.
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert_literal("a", "p", "apple")])?;
    let mut driver = Driver::new(&applier, unbounded());
    driver.step = CommitStep::new(unbounded(), applier.quads().lists().applied_ts())
        .with_catalog(&[catalog_of_ns(1)]);
    let mut declaration = begin(&applier)?.with_catalog(1);
    write(&mut declaration, vec![term_index("p")])?;
    let mut later = begin(&applier)?.with_catalog(1);
    write(&mut later, vec![insert_literal("c", "p", "pear")])?;
    driver.submit(1, prepare(declaration)?)?;
    driver.sync()?;
    assert!(applier.rebuilding(), "the declaration started the build");
    driver.close_with(1, vec![catalog_of_ns(2)], Vec::new())?;
    driver.submit(2, prepare(later)?)?;
    driver.sync()?;
    let mut retry = begin(&applier)?.with_catalog(2);
    write(&mut retry, vec![insert_literal("c", "p", "pear")])?;
    driver.submit(3, prepare(retry)?)?;
    driver.sync()?;
    applier.executor().run_until_idle();
    assert!(!applier.rebuilding());
    w.replies = driver.replies.clone();
    w.ids = indexed_term(&applier, "p", "pear")?;
    w.expected_ids = vec![id_of(&applier, "c")?];
    Ok(())
}

/// Version `version` of the catalog of `ns`, namespace 0.
fn catalog_of_ns(version: u64) -> ultragraphdb_cluster_sequencer::CatalogVersion {
    ultragraphdb_cluster_sequencer::CatalogVersion::new(
        ultragraphdb_core_ids::Namespace::new(NS),
        version,
    )
}

#[then("the later transaction aborts with a conflict, and its retry is indexed by the build")]
fn later_aborts(w: &mut Steps) -> TestResult {
    assert!(
        reply_to(&w.replies, 1)?.outcome().is_ok(),
        "{:?}",
        w.replies
    );
    assert_eq!(
        reply_to(&w.replies, 2)?.outcome(),
        &Err(CommitError::conflict(ConflictKind::Catalog))
    );
    assert!(
        reply_to(&w.replies, 3)?.outcome().is_ok(),
        "{:?}",
        w.replies
    );
    assert_eq!(w.ids, w.expected_ids);
    Ok(())
}

#[when(
    "a transaction executes against the catalog of `ns`, and a declaration of `ns` commits before it"
)]
fn old_catalog(w: &mut Steps) -> TestResult {
    // The declaration commits as version 2 of `ns`, which a close announces before the stale transaction, executed
    // against version 1, is decided.
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert("z", "seen", "age")])?;
    let mut driver = Driver::new(&applier, unbounded());
    driver.step = CommitStep::new(unbounded(), applier.quads().lists().applied_ts())
        .with_catalog(&[catalog_of_ns(1)]);
    let mut stale = begin(&applier)?.with_catalog(1);
    write(&mut stale, vec![insert_literal("c", "age", "old")])?;
    let mut declaration = begin(&applier)?.with_catalog(1);
    let integer = PredicateDeclaration::new(ex("age")).with_datatype(NamedNode::new_unchecked(
        "http://www.w3.org/2001/XMLSchema#integer",
    ));
    write(
        &mut declaration,
        vec![Change::Declare(Declaration::Predicate(integer))],
    )?;
    driver.submit(1, prepare(declaration)?)?;
    driver.sync()?;
    driver.close_with(1, vec![catalog_of_ns(2)], Vec::new())?;
    driver.submit(2, prepare(stale)?)?;
    driver.sync()?;
    w.replies = driver.replies.clone();
    let mut retry = begin(&applier)?;
    let changes = changeset(vec![insert_literal("c", "age", "old")]);
    w.retry_refused = retry.write(&changes, 0).is_err();
    Ok(())
}

#[then(
    "the transaction aborts with a conflict, and its retry is checked against the new declaration"
)]
fn stale_aborts(w: &mut Steps) -> TestResult {
    assert!(
        reply_to(&w.replies, 1)?.outcome().is_ok(),
        "{:?}",
        w.replies
    );
    assert_eq!(
        reply_to(&w.replies, 2)?.outcome(),
        &Err(CommitError::conflict(ConflictKind::Catalog))
    );
    assert!(w.retry_refused, "a string is no integer");
    Ok(())
}

#[when("a transaction inserts `<a> <p> <b>` and a drop of `p` commits after its read version")]
fn write_into_drop(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    w.version = setup(&applier, vec![insert("x", "p", "y"), insert("x", "q", "y")])?;
    let mut driver = Driver::new(&applier, unbounded());
    let mut txn = begin(&applier)?;
    write(&mut txn, vec![insert("a", "p", "b")])?;
    let p = ex("p");
    let drop = drop_of(&applier, DropRequest::Predicate(p.as_ref()), 0, 0)?;
    driver.submit(1, drop)?;
    driver.submit(2, prepare(txn)?)?;
    driver.sync()?;
    driver.sync()?;
    w.replies = driver.replies.clone();
    w.held = held(&applier, "x", "p")?;
    w.held.extend(held(&applier, "a", "p")?);
    w.statements = vec![statements_of(&applier, "p")?, statements_of(&applier, "q")?];
    Ok(())
}

#[then("the transaction aborts, and nothing of `p` is held after the drop")]
fn dropped_nothing_held(w: &mut Steps) -> TestResult {
    assert_eq!(
        outcomes(&w.replies),
        vec![
            (1, Ok(version(1, 0))),
            (2, Err(CommitError::conflict(ConflictKind::Drop)))
        ],
        "the drop alone, then the transaction"
    );
    assert!(w.held.is_empty(), "{:?}", w.held);
    assert_eq!(w.statements, vec![0, 1], "p dropped, q kept");
    Ok(())
}

#[when(
    "a term index on `name` is being built and a transaction inserting `<c> <name> \"pear\"` commits during the build"
)]
fn during_build(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert_literal("a", "name", "apple")])?;
    let mut driver = Driver::new(&applier, unbounded());
    let mut declaration = begin(&applier)?;
    write(&mut declaration, vec![term_index("name")])?;
    driver.submit(1, prepare(declaration)?)?;
    driver.sync()?;
    assert!(applier.rebuilding(), "the build runs");
    let mut txn = begin(&applier)?;
    write(&mut txn, vec![insert_literal("c", "name", "pear")])?;
    driver.submit(2, prepare(txn)?)?;
    driver.sync()?;
    assert!(applier.rebuilding(), "committed during the build");
    applier.executor().run_until_idle();
    assert!(!applier.rebuilding(), "swapped in");
    w.replies = driver.replies.clone();
    w.ids = indexed_term(&applier, "name", "pear")?;
    w.expected_ids = vec![id_of(&applier, "c")?];
    Ok(())
}

#[then("once the build is swapped in, the term index of `name` lists `c` under `pear`")]
fn in_the_built_index(w: &mut Steps) -> TestResult {
    assert!(
        w.replies.iter().all(|r| r.outcome().is_ok()),
        "{:?}",
        w.replies
    );
    assert_eq!(w.ids, w.expected_ids);
    Ok(())
}

// Requirement: Rollups take no version from the commit step.

#[when("a list with deltas at 10 and 12 is rolled up")]
fn rolled_up(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    // Applied on their own, the set-ups take the store's next even versions from 2: fillers at 2 to 8, then `a`'s list
    // of `knows` at 10 and 12.
    for filler in ["f2", "f4", "f6", "f8"] {
        let _version: u64 = setup(&applier, vec![insert(filler, "knows", "b")])?;
    }
    for object in ["c", "d"] {
        w.versions
            .push(setup(&applier, vec![insert("a", "knows", object)])?);
    }
    let mut driver = Driver::new(&applier, unbounded());
    let key = forward(&applier, id_of(&applier, "a")?, "knows")?.to_bytes();
    let lists = applier.quads().lists();
    let deltas: Vec<u64> = lists
        .store()
        .contents()
        .versions(&key)
        .iter()
        .map(|v| v.version())
        .collect();
    assert_eq!(deltas, vec![12, 10], "the deltas, newest first");
    assert!(lists.rollup_key(&key).context("rolled up")?);
    w.versions = vec![lists.store().contents().versions(&key)[0].version()];
    let mut next = begin(&applier)?;
    write(&mut next, vec![insert("e", "knows", "b")])?;
    driver.submit(9, prepare(next)?)?;
    driver.sync()?;
    w.replies = driver.replies.clone();
    Ok(())
}

#[then("its complete version is written at 13, and the next commit's version is even")]
fn between_versions(w: &mut Steps) -> TestResult {
    assert_eq!(w.versions, vec![13]);
    let next = reply_to(&w.replies, 9)?
        .outcome()
        .clone()
        .context("committed")?;
    assert_eq!(next % 2, 0);
    // After the store's version 12, of epoch 0: epoch 1, sequence 0.
    assert_eq!(next, version(1, 0));
    Ok(())
}

// Sequencer: Partitions learn each closed epoch and stamp the open one, the partition's side. The sequencer's own
// delivery and catch-up are `ultragraphdb_cluster_sequencer`'s (group 5); here the test delivers its closes.

#[when("epochs 1 to 7 are delivered to a partition and it commits a transaction")]
fn seven_delivered(w: &mut Steps) -> TestResult {
    // A partition opened on an empty store: its latest delivered close is epoch 0 until closes arrive.
    let mut step = CommitStep::new(unbounded(), 0);
    for delivered in 1..=7 {
        let _out = deliver(&mut step, delivered)?;
    }
    let _round = commit(&mut step, 1, writer(1)?, 0);
    w.versions = committed(run(&mut step, Input::Synced, 0).replies())?;
    Ok(())
}

#[then("the commit's version carries epoch 8")]
fn carries_epoch_8(w: &mut Steps) {
    assert_eq!(w.versions.len(), 1);
    assert_eq!(w.versions[0] >> 22, 8);
}

#[when(
    "epochs 1 to 7 are delivered to a partition, the sequencer has closed epoch 8, and its close has not yet reached the partition"
)]
fn close_8_in_transit(w: &mut Steps) -> TestResult {
    let mut step = CommitStep::new(unbounded(), 0);
    for delivered in 1..=7 {
        let _out = deliver(&mut step, delivered)?;
    }
    // The sequencer's close of 8 is on its way: nothing reaches the step until the test delivers it.
    for ticket in 1..=2 {
        let _round = commit(&mut step, ticket, writer(ticket)?, 0);
        w.versions
            .extend(committed(run(&mut step, Input::Synced, 0).replies())?);
    }
    let _delivered = deliver(&mut step, 8)?;
    let _round = commit(&mut step, 3, writer(3)?, 0);
    w.versions
        .extend(committed(run(&mut step, Input::Synced, 0).replies())?);
    Ok(())
}

#[then(
    "the partition's commits still carry epoch 8, and its first commit after the close of 8 arrives carries epoch 9"
)]
fn epoch_8_then_9(w: &mut Steps) {
    let epochs: Vec<u64> = w.versions.iter().map(|v| v >> 22).collect();
    assert_eq!(epochs, vec![8, 8, 9]);
}

#[when("a partition holds an unapplied round of epoch 8 and the close of epoch 8 is delivered")]
fn unapplied_round_of_8(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    w.version = setup(&applier, vec![insert("a", "knows", "b")])?;
    let mut driver = driver_at(&applier, 1, 7)?;
    let mut txn = begin(&applier)?;
    write(&mut txn, vec![insert("c", "knows", "d")])?;
    driver.submit(1, prepare(txn)?)?;
    driver.close(8)?;
    w.epochs.push(driver.step.closed().get());
    w.versions.push(applier.quads().lists().applied_ts());
    driver.sync()?;
    w.epochs.push(driver.step.closed().get());
    w.versions.push(applier.quads().lists().applied_ts());
    Ok(())
}

#[then(
    "the partition reports epoch 8 closed only after that round is applied, and a snapshot read at epoch 8 waits until then"
)]
fn closed_after_apply(w: &mut Steps) {
    // While the round is unapplied the partition has closed only 7, so a snapshot read at 8, which waits for the
    // partition to have closed 8 (quad-reads), waits; once the round is applied at (8, 0), 8 is closed.
    assert_eq!(w.epochs, vec![7, 8]);
    assert_eq!(w.versions, vec![w.version, version(8, 0)]);
}

#[when("a partition loses its connection after epoch 3 and reconnects after epoch 9 closed")]
fn reconnects(w: &mut Steps) -> TestResult {
    // The partition's half: it takes the closes it missed in order. The sequencer sending them from its log is
    // `ultragraphdb_cluster_sequencer`'s, bound in its own suite (group 5).
    let mut caught_up = step_at(0, 3)?;
    for delivered in 4..=9 {
        let out = deliver(&mut caught_up, delivered)?;
        w.checks
            .push(!caught_up.is_stopped() && out.replies().is_empty());
    }
    w.epochs.push(caught_up.delivered().get());
    let out = commit(&mut caught_up, 1, writer(1)?, 0);
    w.versions
        .extend(out.append().map(|(r, _)| r.first_version()));
    // A later close arriving before the ones missed is refused, past a gap.
    let mut gapped = step_at(0, 3)?;
    let _out = deliver(&mut gapped, 10)?;
    w.errors.extend(gapped.fault().cloned());
    Ok(())
}

#[then("it receives epochs 4 to 9 in order before any later close")]
fn caught_up_in_order(w: &mut Steps) {
    assert_eq!(w.checks, vec![true; 6]);
    assert_eq!(w.epochs, vec![9]);
    assert_eq!(w.versions, vec![version(10, 0)]);
    assert_eq!(w.errors, vec![CommitError::close_out_of_order(3, 10)]);
}

// Transactions: Transactions read a snapshot at the visibility watermark.

#[when(
    "a read-only transaction at snapshot level begins while epoch 20 is the latest close, and one partition it reads has closed only epoch 19"
)]
fn snapshot_at_20(w: &mut Steps) -> TestResult {
    // The partition's half: the epoch it reports closed, which a snapshot read at epoch 20 waits for. The read itself,
    // at its level across partitions, is compute's, bound with the roles (group 9). The partition's latest delivered
    // close is 19, and a round of epoch 20 is in flight when the close of 20 reaches it.
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert("a", "knows", "b")])?;
    let mut driver = driver_at(&applier, 1, 19)?;
    w.epochs.push(driver.step.closed().get());
    let mut txn = begin(&applier)?;
    write(&mut txn, vec![insert("c", "knows", "d")])?;
    driver.submit(1, prepare(txn)?)?;
    driver.close(20)?;
    w.epochs.push(driver.step.closed().get());
    driver.sync()?;
    w.epochs.push(driver.step.closed().get());
    w.replies = driver.replies.clone();
    w.held = held(&applier, "c", "knows")?;
    Ok(())
}

#[then("its read answers once that partition has closed epoch 20, at epoch 20")]
fn answers_at_20(w: &mut Steps) {
    // Closed 19 before the close of 20 and while its round of 20 is unapplied: the read waits; then 20.
    assert_eq!(w.epochs, vec![19, 19, 20]);
    // The commit of epoch 20 is applied, and a read at epoch 20 holds it.
    assert_eq!(outcomes(&w.replies), vec![(1, Ok(version(20, 0)))]);
    assert_eq!(w.held, vec![Term::from(ex("d"))]);
}

// Transactions: Transactions are strictly serializable.

#[when(
    "two transactions each read `on-call` of `alice` and `bob`, both `true`, and each sets a different one to `false`, concurrently, all four facts in one partition"
)]
fn write_skew(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    w.version = setup(
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
        let _read: Vec<Term> = read(txn, "alice", "on-call")?;
        let _read_bob: Vec<Term> = read(txn, "bob", "on-call")?;
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
    w.replies = driver.replies.clone();
    Ok(())
}

#[then("one of them commits and the other aborts with a conflict")]
fn one_commits(w: &mut Steps) -> TestResult {
    // In round order: the first alone at (1, 0); the second read alice's list the first rewrote.
    assert_eq!(
        outcomes(&w.replies),
        vec![
            (1, Ok(version(1, 0))),
            (2, Err(CommitError::conflict(ConflictKind::List)))
        ]
    );
    Ok(())
}

// Transactions: What a transaction's reads record and what conflicts with them.

#[when("two transactions concurrently insert `<a> <knows> <b>` and `<a> <knows> <c>`")]
fn two_objects(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(
        &applier,
        vec![
            insert("a", "knows", "z"),
            insert("z", "knows", "b"),
            insert("z", "knows", "c"),
        ],
    )?;
    let mut driver = Driver::new(&applier, unbounded());
    let txns: Vec<_> = ["b", "c"]
        .iter()
        .map(|object| {
            let mut txn = begin(&applier)?;
            write(&mut txn, vec![insert("a", "knows", object)])?;
            prepare(txn)
        })
        .collect::<TestResult<Vec<_>>>()?;
    for (ticket, prepared) in (1..).zip(txns) {
        driver.submit(ticket, prepared)?;
    }
    driver.sync()?;
    driver.sync()?;
    w.replies = driver.replies.clone();
    w.held = held(&applier, "a", "knows")?;
    Ok(())
}

#[then("both commit, and `a` knows `b` and `c`")]
fn both_objects_held(w: &mut Steps) {
    // The first alone at (1, 0), the second in the next round at (1, 2).
    assert_eq!(
        outcomes(&w.replies),
        vec![(1, Ok(version(1, 0))), (2, Ok(version(1, 2)))]
    );
    for object in ["b", "c"] {
        assert!(w.held.contains(&Term::from(ex(object))), "{:?}", w.held);
    }
}

#[when(
    "two transactions in `<a>`'s partition concurrently insert `<a> <knows> <b>`, which is not held, `<b>` new and homed in another partition"
)]
fn same_new_statement(w: &mut Steps) -> TestResult {
    // The partition's half: `<a>`'s partition decides the two inserts. `<b>`'s registrations at its home, which give
    // both one ID (term-dictionary, "A reference to another partition's entity registers at its home"), are the
    // partitions' (group 7); here one store stands for both, its dictionary naming `<b>` once.
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert("a", "knows", "z")])?;
    let mut driver = Driver::new(&applier, unbounded());
    let txns: Vec<_> = (0..2)
        .map(|_| {
            let mut txn = begin(&applier)?;
            write(&mut txn, vec![insert("a", "knows", "b")])?;
            prepare(txn)
        })
        .collect::<TestResult<Vec<_>>>()?;
    for (ticket, prepared) in (1..).zip(txns) {
        driver.submit(ticket, prepared)?;
    }
    driver.sync()?;
    driver.sync()?;
    w.replies = driver.replies.clone();
    w.ids = vec![id_of(&applier, "b")?];
    let mut retry = begin(&applier)?;
    w.held = read(&mut retry, "a", "knows")?;
    Ok(())
}

#[then(
    "both registrations of `<b>` yield one ID and neither aborts; one transaction commits and the other aborts with a conflict, and its retry finds the statement held"
)]
fn one_of_the_same_commits(w: &mut Steps) {
    assert_eq!(w.ids.len(), 1, "the dictionary names `<b>` once");
    assert_eq!(w.replies.len(), 2);
    // The first alone at (1, 0); the second, in the next round, inserted the statement the first did.
    assert_eq!(reply_outcome(&w.replies, 1), Some(Ok(version(1, 0))));
    assert!(
        matches!(
            reply_outcome(&w.replies, 2),
            Some(Err(CommitError(CommitErrorEnum::Conflict { .. })))
        ),
        "{:?}",
        w.replies
    );
    assert!(w.held.contains(&Term::from(ex("b"))), "{:?}", w.held);
}

/// The outcome of the reply to `ticket`, if one was sent.
fn reply_outcome(replies: &[Reply], ticket: u64) -> Option<Result<u64, CommitError>> {
    replies
        .iter()
        .find(|r| r.ticket() == ticket)
        .map(|r| r.outcome().clone())
}

/// The key of the `int` index of `age` holding `value`, `age` standing as the test ID of sequence 3: Dgraph's int
/// tokenizer, identifier 0x06 (`tok/tok.go`), its token a sign byte, 1 for a value at or above 0, and the value's 8
/// big-endian bytes.
fn age_key(value: i64) -> TestResult<Vec<u8>> {
    let mut token = vec![u8::from(value >= 0)];
    token.extend_from_slice(&value.to_be_bytes());
    Ok(QuadKey::ValueIndex {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: crate::utils::proposal_double::id(3)?,
        tokenizer: 0x06,
        token,
    }
    .to_bytes())
}

/// The commit step deciding a scan of `age`'s index from 30 to 40, ending past 40's key, with the write of
/// `<x> <seen> true`, against an insert of `<c> <age> {inserted}` writing its index key and committed first, above
/// the scan's read version: the replies. The commit step's half; that a scan records its range and an insert writes
/// its index key is the store's, bound by `ultragraphdb_store_apply`'s read-set scenarios.
fn scan_against(inserted: i64) -> TestResult<Vec<Reply>> {
    let mut end = age_key(40)?;
    end.push(0);
    let scanner = proposal(
        0,
        &Reads::range(&age_key(30)?, Some(&end)),
        &Writes::point(b"x-seen"),
    )?;
    let inserter = proposal(0, &Reads::default(), &Writes::point(&age_key(inserted)?))?;
    let mut step = CommitStep::new(unbounded(), 0);
    let mut replies = commit(&mut step, 1, inserter, 0).replies().to_vec();
    replies.extend(commit(&mut step, 2, scanner, 0).replies().iter().cloned());
    for _ in 0..2 {
        replies.extend(run(&mut step, Input::Synced, 0).replies().iter().cloned());
    }
    Ok(replies)
}

#[when(
    "a transaction scans the `int` index of `age` for values from 30 to 40 and writes `<x> <seen> true`, and a transaction inserting `<c> <age> 35` commits after its read version"
)]
fn scan_against_35(w: &mut Steps) -> TestResult {
    w.replies = scan_against(35)?;
    Ok(())
}

#[then("the scanning transaction aborts with a conflict")]
fn scanner_aborts(w: &mut Steps) {
    assert_eq!(
        outcomes(&w.replies),
        vec![
            (1, Ok(version(1, 0))),
            (2, Err(CommitError::conflict(ConflictKind::Range)))
        ]
    );
}

#[when(
    "a transaction scans the `int` index of `age` for values from 30 to 40 and writes `<x> <seen> true`, and a transaction inserting `<c> <age> 50` commits after its read version"
)]
fn scan_against_50(w: &mut Steps) -> TestResult {
    w.replies = scan_against(50)?;
    Ok(())
}

#[then("the scanning transaction commits")]
fn scanner_commits(w: &mut Steps) {
    assert_eq!(
        outcomes(&w.replies),
        vec![(1, Ok(version(1, 0))), (2, Ok(version(1, 2)))]
    );
}

#[when(
    "a transaction reads `<new>`'s statements, finds none, and writes `<x> <seen> true`, and a transaction in the same partition writing `<new> <name> \"N\"` commits after its read version"
)]
fn absent_then_written(w: &mut Steps) -> TestResult {
    // The commit step's half, `<new>` the test ID of sequence 7: the reader's read set holds it found absent, the
    // writer's record a statement of it. Recording the read as an absent term is the transaction's.
    let reader = proposal(0, &Reads::absent(7), &Writes::point(b"x-seen"))?;
    let writer = proposal(0, &Reads::default(), &Writes::term(7))?;
    let mut step = CommitStep::new(unbounded(), 0);
    w.replies = commit(&mut step, 1, writer, 0).replies().to_vec();
    w.replies
        .extend(commit(&mut step, 2, reader, 0).replies().iter().cloned());
    for _ in 0..2 {
        w.replies
            .extend(run(&mut step, Input::Synced, 0).replies().iter().cloned());
    }
    Ok(())
}

#[then("the reading transaction aborts with a conflict naming an absent term")]
fn absent_term_conflict(w: &mut Steps) {
    assert_eq!(
        outcomes(&w.replies),
        vec![
            (1, Ok(version(1, 0))),
            (2, Err(CommitError::conflict(ConflictKind::AbsentTerm)))
        ]
    );
}

#[when(
    "a transaction in `<new>`'s home reads `<new>`'s statements, finds none, and writes `<x> <seen> true`, and a registration from another partition creates `<new>`'s dictionary entry there after the read version, with no statement of `<new>` committed in the home"
)]
fn absent_then_registered(w: &mut Steps) -> TestResult {
    // The home's half, `<new>` the test ID of sequence 7: a registration from bucket group 3, another partition's, which
    // the home accepted, takes a version of its own above the reader's read version. Issuing it from the referrer and
    // the home's get-or-create are the partitions' (group 7).
    let reader = proposal(0, &Reads::absent(7), &Writes::point(b"x-seen"))?;
    let mut step = CommitStep::new(unbounded(), 0).with_partition(2);
    let registered = Input::Registration {
        ticket: 1,
        record: registration(7, 3, 1)?,
    };
    w.replies = run(&mut step, registered, 0).replies().to_vec();
    w.replies
        .extend(commit(&mut step, 2, reader, 0).replies().iter().cloned());
    for _ in 0..2 {
        w.replies
            .extend(run(&mut step, Input::Synced, 0).replies().iter().cloned());
    }
    Ok(())
}

#[then("the reading transaction commits")]
fn reader_commits(w: &mut Steps) {
    // The registration alone at (1, 0), the reader in the next round at (1, 2).
    assert_eq!(
        outcomes(&w.replies),
        vec![(1, Ok(version(1, 0))), (2, Ok(version(1, 2)))]
    );
}

// Transactions: Exactly-once commits through sessions.

/// Commits `<{subject}> <knows> <b>` through `pipe` as request `request` of `session`, under the request's number as
/// its ticket, in a round of its own.
async fn commit_through(
    pipe: &mut Pipe<'_>,
    applier: &Applier,
    session: u128,
    request: u64,
    subject: &str,
) -> TestResult {
    let mut txn = begin(applier)?;
    write(&mut txn, vec![insert(subject, "knows", "b")])?;
    pipe.commit(request, prepare_in(txn, session, request)?, 0)
        .context("appended")?;
    pipe.sync(0).await.context("synced")?;
    Ok(())
}

#[when("a client commits request 7 of its session, loses the reply, and sends request 7 again")]
async fn retried(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    w.version = setup(&applier, vec![insert("a", "knows", "b")])?;
    let (_storage, log) = open_wal().await?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).context("it opens")?;
    for request in 1..=6u64 {
        commit_through(
            &mut pipe,
            &applier,
            SESSION,
            request,
            &format!("s{request}"),
        )
        .await?;
    }
    pipe.replies_mut().clear();
    let seven = || {
        let mut txn = begin(&applier)?;
        write(&mut txn, vec![insert("x", "knows", "y")])?;
        prepare_in(txn, SESSION, 7)
    };
    pipe.commit(7, seven()?, 0).context("appended")?;
    pipe.sync(0).await.context("synced")?;
    let first = pipe.replies_mut().pop().context("the reply, lost")?;
    let applied = applier.quads().lists().applied_ts();
    pipe.commit(8, seven()?, 0).context("answered")?;
    pipe.sync(0).await.context("synced")?;
    w.replies = vec![first];
    w.replies.append(pipe.replies_mut());
    w.versions = vec![applied, applier.quads().lists().applied_ts()];
    w.held = held(&applier, "x", "knows")?;
    Ok(())
}

#[then("it receives the same reply and commit version, and the transaction's writes are held once")]
fn same_reply(w: &mut Steps) -> TestResult {
    // One round each: requests 1 to 6 at sequences 0 to 10 of epoch 1, request 7 at (1, 12); the retry, under ticket 8,
    // takes no version and is answered with 7's reply, its token included.
    assert_eq!(
        outcomes(&w.replies),
        vec![(7, Ok(version(1, 12))), (8, Ok(version(1, 12)))]
    );
    assert!(w.replies[0].token().is_some());
    assert_eq!(w.replies[0].token(), w.replies[1].token());
    assert_eq!(w.versions[0], w.versions[1], "the retry applies nothing");
    assert_eq!(w.held.len(), 1);
    Ok(())
}

#[when(
    "a session commits request 1 in partition 1, request 2 in partition 2 and request 3 in partition 1, then loses the reply to request 3 and sends it again on a new connection"
)]
fn two_partitions_session(w: &mut Steps) -> TestResult {
    let (store_1, store_2) = (applier()?, applier()?);
    let mut one = driver_at(&store_1, 1, 0)?;
    let mut two = driver_at(&store_2, 2, 0)?;
    commit_in(&mut one, &store_1, SESSION, 1, "r1")?;
    commit_in(&mut two, &store_2, SESSION, 2, "r2")?;
    commit_in(&mut one, &store_1, SESSION, 3, "r3")?;
    w.replies = one.replies.clone();
    w.replies.extend(two.replies.iter().cloned());
    // The retry on a new connection, with no transaction open, asks partition 1's table.
    w.stored
        .push(one.step.retry(SESSION, 3).context("the stored reply")?);
    w.sessions.push(one.step.sessions().last(SESSION));
    w.checks = vec![
        held(&store_1, "r1", "knows")?.len() == 1,
        held(&store_2, "r2", "knows")?.len() == 1,
        held(&store_1, "r3", "knows")?.len() == 1,
    ];
    Ok(())
}

#[then(
    "all three commit once, partition 1 accepts request 3 after its last, 1, and the retry receives request 3's stored reply from partition 1"
)]
fn three_commit_once(w: &mut Steps) -> TestResult {
    // Each partition's store starts empty: its first commit at (1, 0), partition 1's second at (1, 2).
    assert_eq!(
        outcomes(&w.replies),
        vec![
            (1, Ok(version(1, 0))),
            (3, Ok(version(1, 2))),
            (2, Ok(version(1, 0)))
        ]
    );
    assert_eq!(w.checks, vec![true, true, true], "each held once");
    assert_eq!(w.sessions, vec![Some((3, version(1, 2)))]);
    let stored = w.stored.first().context("the stored reply")?;
    assert_eq!(stored.version(), version(1, 2));
    assert_eq!(Some(stored.token()), w.replies[1].token());
    assert_eq!(stored.token().partition(), 1);
    Ok(())
}

#[when("a session has committed in partition 1, and the registry evicts it")]
fn evicted_by_the_registry(w: &mut Steps) -> TestResult {
    // The partition's half: its table and the closes it is delivered. The registry's eviction is the metadata group's
    // (group 6), its carriage in the next close the sequencer's (group 5).
    let applier = applier()?;
    let mut driver = driver_at(&applier, 1, 0)?;
    commit_in(&mut driver, &applier, SESSION, 1, "s")?;
    w.sessions.push(driver.step.sessions().last(SESSION));
    // A close carrying no eviction, then the one carrying it.
    driver.close(1)?;
    w.sessions.push(driver.step.sessions().last(SESSION));
    driver.close_with(2, Vec::new(), vec![SESSION])?;
    w.sessions.push(driver.step.sessions().last(SESSION));
    Ok(())
}

#[then(
    "partition 1 keeps the session's entry until the close carrying the eviction is delivered to it, and then purges it"
)]
fn purged_with_the_close(w: &mut Steps) {
    let entry = Some((1, version(1, 0)));
    assert_eq!(w.sessions, vec![entry, entry, None]);
}

#[when(
    "a client sends request 8 of its session again, with no transaction open for it, and the session's last committed request is 7"
)]
async fn never_committed(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert("a", "knows", "b")])?;
    let (_storage, log) = open_wal().await?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).context("it opens")?;
    for request in 1..=7u64 {
        commit_through(
            &mut pipe,
            &applier,
            SESSION,
            request,
            &format!("s{request}"),
        )
        .await?;
    }
    let applied = applier.quads().lists().applied_ts();
    w.errors = vec![pipe.retry(SESSION, 8).refused("not committed")?];
    w.versions = vec![applied, applier.quads().lists().applied_ts()];
    Ok(())
}

#[then("the request is refused as not committed, `detail` 8, and nothing is committed")]
fn not_committed(w: &mut Steps) -> TestResult {
    let error = w.errors.first().context("refused")?;
    assert_eq!(error, &CommitError::not_committed(8));
    assert_eq!(error.detail(), 8);
    assert_eq!(w.versions[0], w.versions[1], "nothing committed");
    Ok(())
}

// Transactions: Aborts name their reason.

#[when("the commit queue is full and a transaction commits")]
async fn queue_full(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert("a", "knows", "b")])?;
    let (_storage, log) = open_wal().await?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded())
        .context("it opens")?
        .with_queue(2);
    // One round in flight and two waiting fill the queue.
    for (ticket, subject) in [(1, "p"), (2, "q"), (3, "r"), (4, "late")] {
        let mut txn = begin(&applier)?;
        write(&mut txn, vec![insert(subject, "knows", "b")])?;
        pipe.commit(ticket, prepare(txn)?, 0)
            .context("taken or refused")?;
    }
    w.replies = std::mem::take(pipe.replies_mut());
    pipe.sync(0).await.context("synced")?;
    pipe.sync(0).await.context("synced")?;
    w.held = held(&applier, "late", "knows")?;
    Ok(())
}

#[then("the commit fails with a retryable busy error, and nothing it wrote is held")]
fn busy(w: &mut Steps) {
    assert_eq!(outcomes(&w.replies), vec![(4, Err(CommitError::busy(2)))]);
    assert!(CommitError::busy(2).is_retryable());
    assert!(w.held.is_empty());
}

#[when("a transaction's write set grows past the Welcome's `max_write_set` and it commits")]
fn past_the_bound(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert("a", "knows", "b")])?;
    let mut txn = begin(&applier)?;
    write(
        &mut txn,
        vec![insert_literal("big", "note", &"x".repeat(MAX_WRITE_SET))],
    )?;
    w.refusal = txn.commit(0, 0, &[], 0).err();
    w.held = held(&applier, "big", "note")?;
    Ok(())
}

#[then(
    "the commit fails with a too-large error that is not marked retryable, its `detail` the bound, and nothing it wrote is held"
)]
fn past_the_bound_refused(w: &mut Steps) -> TestResult {
    let error = w
        .refusal
        .as_ref()
        .context("the transaction is refused, not prepared")?;
    assert!(
        matches!(
            error.0,
            TxnErrorEnum::WriteSetTooLarge {
                limit: MAX_WRITE_SET,
                ..
            }
        ),
        "{error:?}"
    );
    assert_eq!(MAX_WRITE_SET, 1_040_384, "the Welcome's default bound");
    assert!(!error.is_retryable());
    assert!(w.held.is_empty());
    Ok(())
}

/// Two partitions, each owning half the buckets.
fn two_partitions() -> TestResult<PlacementMap> {
    PlacementMap::even(2, NodeId::new(1)).context("the placement")
}

fn partition(n: u32) -> TestResult<PartitionId> {
    PartitionId::new(n).context("the partition")
}

/// The name `s{i}` of the first `i` whose IRI's bucket `placement` gives to `partition` at epoch 0: the placement map
/// and the bucket derivation are the oracle.
fn owned_by(placement: &PlacementMap, partition: u32) -> TestResult<String> {
    for i in 0..1_000 {
        let name = format!("s{i}");
        let iri = ex(&name);
        let key = CanonicalKey::iri(iri.as_str()).context("the key")?;
        let bucket = Bucket::derive(Namespace::new(NS), TypePrefix::RESOURCE, &key);
        if placement.partition_of(bucket, Epoch::ZERO).get() == partition {
            return Ok(name);
        }
    }
    Err(TestError::new(format!("no name of partition {partition}")))
}

#[when(
    "a transaction bound to partition 1 writes a statement whose subject's bucket partition 2 owns"
)]
fn second_partitions_subject(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert("a", "knows", "b")])?;
    let placement = two_partitions()?;
    let other = owned_by(&placement, 2)?;
    let mut txn = begin(&applier)?.bind(partition(1)?, &placement);
    w.refusal = txn
        .write(&changeset(vec![insert(&other, "knows", "b")]), 0)
        .err();
    w.held = held(&applier, &other, "knows")?;
    Ok(())
}

#[then(
    "it aborts with a cross-partition error naming partitions 1 and 2, not marked retryable, and nothing it wrote is held"
)]
fn cross_partition_write(w: &mut Steps) -> TestResult {
    let error = w.refusal.as_ref().context("the write is refused")?;
    assert_eq!(error, &TxnError::cross_partition(1, 2));
    assert!(!error.is_retryable());
    assert!(w.held.is_empty());
    Ok(())
}

// Transactions: Commit-time operations. One partition owns every bucket here, so every claim is local; a claim another
// partition owns is the scenario's own.

/// The unique declaration of `epi.id`, committed as set-up.
fn unique_id(applier: &Applier) -> TestResult {
    let declared = PredicateDeclaration::new(ex("epi.id")).with_unique(true);
    let _version: u64 = setup(
        applier,
        vec![Change::Declare(Declaration::Predicate(declared))],
    )?;
    Ok(())
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

/// The insert-if-absent of a node with `epi.id` `value`.
fn insert_if_absent(value: &str) -> Operation {
    Operation::InsertIfAbsent {
        graph: GraphName::DefaultGraph,
        predicate: ex("epi.id"),
        value: Literal::new_simple_literal(value).into(),
        properties: Vec::new(),
    }
}

/// Commits `txns` through a pipeline over a fresh log as concurrent transactions do: the first alone, half the rest
/// waiting while it syncs and decided over it before it is applied, then the others while that round syncs, every round
/// synced. The replies in ticket order.
async fn commit_concurrently(applier: &Applier, txns: Vec<Prepared>) -> TestResult<Vec<Reply>> {
    let (_storage, log) = open_wal().await?;
    let mut pipe: Pipe<'_> = Pipeline::new(applier, log, unbounded()).context("it opens")?;
    let n = txns.len();
    for (ticket, prepared) in txns.into_iter().enumerate() {
        pipe.commit(ticket as u64, prepared, 0)
            .context("appended")?;
        // The first round stays in flight while half the rest wait: they are decided over it, unapplied.
        if ticket == n / 2 {
            pipe.sync(0).await.context("synced")?;
        }
    }
    for _ in 0..3 {
        pipe.sync(0).await.context("synced")?;
    }
    let mut replies = std::mem::take(pipe.replies_mut());
    replies.sort_by_key(Reply::ticket);
    Ok(replies)
}

/// The statements of predicate `<name>` the store holds, by its statistics.
fn statements_of(applier: &Applier, name: &str) -> TestResult<i64> {
    let Some(predicate) = applier
        .quads()
        .id_of(NS, ex(name).as_str())
        .context("the dictionary reads")?
    else {
        return Ok(0);
    };
    let watermark = applier.quads().lists().applied_ts();
    Ok(applier
        .quads()
        .statistics(NS, DEFAULT_GRAPH, predicate, watermark)
        .context("the statistics read")?
        .statements())
}

/// The encoded objects of the list `key` at the watermark.
fn objects_of(applier: &Applier, key: &QuadKey) -> TestResult<Vec<EncodedTerm>> {
    let watermark = applier.quads().lists().applied_ts();
    Ok(applier
        .quads()
        .objects(&key.to_bytes(), watermark)
        .context("the list reads")?
        .into_iter()
        .map(|(object, _)| object)
        .collect())
}

/// The forward list of `subject`'s `<predicate>`.
fn forward(applier: &Applier, subject: SystemId, predicate: &str) -> TestResult<QuadKey> {
    Ok(QuadKey::Forward {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: id_of(applier, predicate)?,
        subject,
    })
}

/// The terms of the forward list of `<predicate>` of the node named `iri`, at the watermark.
fn node_terms(applier: &Applier, iri: &str, predicate: &str) -> TestResult<Vec<Term>> {
    let key = forward(applier, node_id(applier, iri)?, predicate)?;
    applier
        .quads()
        .list_terms(NS, &key.to_bytes(), applier.quads().lists().applied_ts())
        .context("the node reads")
}

/// The system ID the store holds for the node named `iri`, an IRI or a skolem IRI.
fn node_id(applier: &Applier, iri: &str) -> TestResult<SystemId> {
    applier
        .quads()
        .id_of(NS, iri)
        .context("the dictionary reads")?
        .context("the node's IRI is known")
}

/// The IRI `reply`'s one result names, from its term table.
fn node_named(reply: &Reply) -> TestResult<String> {
    assert_eq!(reply.results().len(), 1, "{reply:?}");
    let index = reply.results()[0]
        .node()
        .context("the result names a node")?;
    let term = reply
        .terms()
        .get(index)
        .context("the term table holds it")?;
    let RequestTerm::Iri(text) = term else {
        return Err(TestError::new(format!("a node named by {term:?}")));
    };
    Ok(text.clone())
}

/// The one IRI every reply's single result names, and how many replies said they created it.
fn one_node(replies: &[Reply]) -> TestResult<(String, usize)> {
    let named = replies
        .iter()
        .map(node_named)
        .collect::<TestResult<Vec<String>>>()?;
    let first = named.first().context("a reply")?.clone();
    assert!(named.iter().all(|n| *n == first), "{named:?}");
    let created = replies
        .iter()
        .filter(|r| r.results().first().is_some_and(OperationResult::created))
        .count();
    Ok((first, created))
}

#[when(
    "eight transactions concurrently insert-if-absent a node with `epi.id` `\"x\"`, declared unique, all in the partition owning the bucket of `\"x\"`"
)]
async fn insert_if_absent_eight(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    unique_id(&applier)?;
    let txns: Vec<_> = (0..8)
        .map(|_| {
            let mut txn = begin(&applier)?;
            txn.operate(insert_if_absent("x"), 0).context("asked")?;
            prepare(txn)
        })
        .collect::<TestResult<Vec<_>>>()?;
    w.replies = commit_concurrently(&applier, txns).await?;
    w.statements = vec![statements_of(&applier, "epi.id")?];
    let (node, _) = one_node(&w.replies)?;
    w.terms = vec![node_terms(&applier, &node, "epi.id")?];
    Ok(())
}

#[then("all eight commit, all receive the same IRI, and one node holds `epi.id` `\"x\"`")]
fn one_node_eight(w: &mut Steps) -> TestResult {
    assert_eq!(w.replies.len(), 8);
    assert!(
        w.replies.iter().all(|r| r.outcome().is_ok()),
        "{:?}",
        w.replies
    );
    let (_, created) = one_node(&w.replies)?;
    assert_eq!(created, 1, "one reply created the node");
    assert_eq!(w.statements, vec![1], "one statement of epi.id");
    assert_eq!(
        w.terms,
        vec![vec![Term::from(Literal::new_simple_literal("x"))]]
    );
    Ok(())
}

#[when(
    "eight transactions each create a memory `m1` to `m8` and concurrently upsert-and-link the entity with `epi.id` `\"x\"`, declared unique, each linking its memory to it by `HAS_ENTITY`, all in the partition owning the bucket of `\"x\"`"
)]
async fn upsert_and_link_eight(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    unique_id(&applier)?;
    let _version: u64 = setup(&applier, vec![insert_literal("m0", "kind", "memory")])?;
    let memories: Vec<String> = (1..=8).map(|i| format!("m{i}")).collect();
    let txns: Vec<_> = memories
        .iter()
        .map(|memory| {
            let mut txn = begin(&applier)?;
            write(&mut txn, vec![insert_literal(memory, "kind", "memory")])?;
            txn.operate(upsert("x", Vec::new(), &[memory]), 0)
                .context("asked")?;
            prepare(txn)
        })
        .collect::<TestResult<Vec<_>>>()?;
    w.replies = commit_concurrently(&applier, txns).await?;
    w.statements = vec![statements_of(&applier, "epi.id")?];
    let (node, _) = one_node(&w.replies)?;
    let node = node_id(&applier, &node)?;
    w.objects = memories
        .iter()
        .map(|m| {
            objects_of(
                &applier,
                &forward(&applier, id_of(&applier, m)?, "HAS_ENTITY")?,
            )
        })
        .collect::<TestResult<_>>()?;
    let reverse = QuadKey::Reverse {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: id_of(&applier, "HAS_ENTITY")?,
        object: EncodedTerm::id(node),
    };
    w.ids = objects_of(&applier, &reverse)?
        .iter()
        .filter_map(EncodedTerm::as_id)
        .collect();
    w.ids.sort_unstable();
    w.expected_ids = memories
        .iter()
        .map(|m| id_of(&applier, m))
        .collect::<TestResult<_>>()?;
    w.expected_ids.sort_unstable();
    w.ids.push(node);
    Ok(())
}

#[then(
    "all eight commit and receive the same IRI, one node holds `epi.id` `\"x\"`, each memory holds `HAS_ENTITY` to that node, and a reverse read of `HAS_ENTITY` for the node returns all eight memories"
)]
fn linked_once(w: &mut Steps) -> TestResult {
    assert_eq!(w.replies.len(), 8);
    assert!(
        w.replies.iter().all(|r| r.outcome().is_ok()),
        "{:?}",
        w.replies
    );
    let (_, created) = one_node(&w.replies)?;
    assert_eq!(created, 1);
    assert_eq!(w.statements, vec![1]);
    let node = w.ids.pop().context("the node")?;
    for objects in &w.objects {
        assert_eq!(objects, &vec![EncodedTerm::id(node)]);
    }
    assert_eq!(w.ids.len(), 8);
    assert_eq!(
        w.ids, w.expected_ids,
        "the reverse read returns the eight memories"
    );
    Ok(())
}

#[when(
    "two transactions concurrently upsert-and-link `epi.id` `\"y\"`, one carrying the label `\"first\"` for creation and the other `\"second\"`, both in the partition owning the bucket of `\"y\"`"
)]
async fn creation_properties(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    unique_id(&applier)?;
    let txns: Vec<_> = ["first", "second"]
        .iter()
        .map(|label| {
            let mut txn = begin(&applier)?;
            let properties = vec![(ex("label"), Literal::new_simple_literal(*label).into())];
            txn.operate(upsert("y", properties, &[]), 0)
                .context("asked")?;
            prepare(txn)
        })
        .collect::<TestResult<Vec<_>>>()?;
    w.replies = commit_concurrently(&applier, txns).await?;
    let (node, _) = one_node(&w.replies)?;
    w.terms = vec![node_terms(&applier, &node, "label")?];
    Ok(())
}

#[then(
    "the node holds the label of the transaction committed first and not the other, and only that transaction's reply says the node was created"
)]
fn created_once(w: &mut Steps) -> TestResult {
    assert!(
        w.replies.iter().all(|r| r.outcome().is_ok()),
        "{:?}",
        w.replies
    );
    let first = w
        .replies
        .iter()
        .min_by_key(|r| r.outcome().clone().unwrap_or(u64::MAX))
        .context("two")?;
    assert_eq!(first.ticket(), 0, "the first ticket committed first");
    assert!(w.replies[0].results()[0].created());
    assert!(!w.replies[1].results()[0].created());
    assert_eq!(
        w.terms,
        vec![vec![Term::from(Literal::new_simple_literal("first"))]]
    );
    Ok(())
}

#[when(
    "a transaction in the partition owning the bucket of `\"z\"` upsert-and-links `epi.id` `\"z\"` and aborts with a conflict on another of its reads"
)]
async fn aborted_upsert(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    unique_id(&applier)?;
    let _version: u64 = setup(&applier, vec![insert("a", "knows", "b")])?;
    let mut txn = begin(&applier)?;
    let _read: Vec<Term> = read(&mut txn, "a", "knows")?;
    write(&mut txn, vec![insert_literal("mz", "kind", "memory")])?;
    txn.operate(upsert("z", Vec::new(), &["mz"]), 0)
        .context("asked")?;
    let mut other = begin(&applier)?;
    write(&mut other, vec![insert("a", "knows", "c")])?;
    w.replies = commit_concurrently(&applier, vec![prepare(other)?, prepare(txn)?]).await?;
    w.statements = vec![
        statements_of(&applier, "epi.id")?,
        statements_of(&applier, "HAS_ENTITY")?,
    ];
    Ok(())
}

#[then("no node holds `epi.id` `\"z\"` and no link of the transaction is held")]
fn nothing_upserted(w: &mut Steps) -> TestResult {
    assert!(w.replies[0].outcome().is_ok(), "{:?}", w.replies);
    assert_eq!(
        w.replies[1].outcome(),
        &Err(CommitError::conflict(ConflictKind::List))
    );
    assert!(w.replies[1].results().is_empty());
    assert_eq!(w.statements, vec![0, 0]);
    Ok(())
}

#[when(
    "a transaction inserts `<a> <seen> true` and compare-and-sets `status` of `a` from `\"open\"` to `\"closed\"` while `status` is `\"done\"`"
)]
async fn failed_cas(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert_literal("a", "status", "done")])?;
    let mut txn = begin(&applier)?;
    write(
        &mut txn,
        vec![Change::Insert {
            graph: GraphName::DefaultGraph,
            subject: ex("a").into(),
            predicate: ex("seen"),
            object: Literal::from(true).into(),
        }],
    )?;
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
    .context("asked")?;
    w.replies = commit_concurrently(&applier, vec![prepare(txn)?]).await?;
    w.terms = vec![held(&applier, "a", "status")?, held(&applier, "a", "seen")?];
    Ok(())
}

#[then(
    "the transaction is refused reporting `\"done\"`, `status` of `a` stays `\"done\"`, and `<a> <seen> true` is not held"
)]
fn cas_refused(w: &mut Steps) -> TestResult {
    let error = w.replies[0].outcome().clone().refused("refused")?;
    assert_eq!(
        error,
        CommitError::operation_failed(Some("\"done\"".to_string()))
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
    Ok(())
}

/// The insert of `<counters> <next-id> 5`.
fn counter_at_5() -> Change {
    Change::Insert {
        graph: GraphName::DefaultGraph,
        subject: ex("counters").into(),
        predicate: ex("next-id"),
        object: Literal::from(5i64).into(),
    }
}

/// A counter add of 1 to `<counters> <next-id>`, its transaction prepared.
fn add_one(applier: &Applier) -> TestResult<Prepared> {
    let mut txn = begin(applier)?;
    txn.operate(
        Operation::CounterAdd {
            graph: GraphName::DefaultGraph,
            subject: ex("counters").into(),
            predicate: ex("next-id"),
            delta: 1,
        },
        0,
    )
    .context("asked")?;
    prepare(txn)
}

#[when("a hundred transactions concurrently add 1 to the counter `next-id`")]
async fn counter_adds(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![counter_at_5()])?;
    let txns: Vec<_> = (0..100)
        .map(|_| add_one(&applier))
        .collect::<TestResult<Vec<_>>>()?;
    w.replies = commit_concurrently(&applier, txns).await?;
    w.terms = vec![held(&applier, "counters", "next-id")?];
    Ok(())
}

#[then("all commit and `next-id` has grown by 100")]
fn grown_by_100(w: &mut Steps) -> TestResult {
    assert_eq!(w.replies.len(), 100);
    assert!(
        w.replies.iter().all(|r| r.outcome().is_ok()),
        "{:?}",
        w.replies
    );
    // 5 at the start, plus 100 adds of 1.
    assert_eq!(w.terms, vec![vec![Term::from(Literal::from(105i64))]]);
    Ok(())
}

#[when("the counter `next-id` holds 5, and two transactions each add 1 to it")]
async fn two_adds(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![counter_at_5()])?;
    w.replies = commit_concurrently(&applier, vec![add_one(&applier)?, add_one(&applier)?]).await?;
    w.terms = vec![held(&applier, "counters", "next-id")?];
    Ok(())
}

#[then("both commit, the one committed first receives 6 and the other 7, and `next-id` holds 7")]
fn six_then_seven(w: &mut Steps) -> TestResult {
    let mut answered: Vec<(u64, i64)> = w
        .replies
        .iter()
        .map(|r| {
            let version = r.outcome().clone().context("committed")?;
            Ok((version, r.results()[0].value()))
        })
        .collect::<TestResult<_>>()?;
    answered.sort_unstable();
    assert_eq!(
        answered.iter().map(|(_, value)| *value).collect::<Vec<_>>(),
        vec![6, 7],
        "in commit order"
    );
    assert_eq!(w.terms, vec![vec![Term::from(Literal::from(7i64))]]);
    Ok(())
}

/// A placement of the fewest partitions, from 2, in which partition 2 owns the bucket the claim of `epi.id` `value`
/// takes, derived from the predicate's ID and the value's encoding (`Bucket::of_value`): the placement map and the
/// bucket derivation are the oracle.
fn owning_value(applier: &Applier, value: &str) -> TestResult<PlacementMap> {
    let predicate = id_of(applier, "epi.id")?;
    let literal = Literal::new_simple_literal(value);
    let encoded = EncodedTerm::literal::<TestError>(literal.as_ref(), |_| {
        Err(TestError::new("a plain literal names no datatype"))
    })?;
    let bucket = Bucket::of_value(Namespace::new(NS), predicate, encoded.term().as_bytes());
    for partitions in 2..=16 {
        let placement = PlacementMap::even(partitions, NodeId::new(1)).context("the placement")?;
        if placement.partition_of(bucket, Epoch::ZERO).get() == 2 {
            return Ok(placement);
        }
    }
    Err(TestError::new(format!(
        "no placement of 2 to 16 partitions gives the bucket of {value:?} to partition 2"
    )))
}

/// Decides `prepared` under ticket 1 in a commit step whose operator is bound to `partition` of `placement`, applying
/// what it admits: the replies.
fn decide_bound(
    applier: &Applier,
    prepared: Prepared,
    partition: PartitionId,
    placement: &PlacementMap,
) -> TestResult<Vec<Reply>> {
    let mut step = CommitStep::new(unbounded(), applier.quads().lists().applied_ts());
    let mut operator = ApplierOperator::new(applier).bound(partition, placement);
    let mut out = StepOutput::new();
    let mut replies = Vec::new();
    for input in [
        Input::Commit(CommitRequest::new(1, prepared)),
        Input::Synced,
    ] {
        step.step_with(input, 0, &mut out, &mut operator);
        let round: Vec<_> = out
            .apply_mut()
            .drain(..)
            .map(|(version, p)| (version, p.into_parts().0))
            .collect();
        if !round.is_empty() {
            let _applied: Vec<Applied> = applier.apply_round(round).context("the round applies")?;
        }
        replies.extend(out.replies().iter().cloned());
    }
    Ok(replies)
}

#[when(
    "`epi.id` is declared unique, partition 2 owns the bucket of `\"x\"`, and a transaction bound to partition 1 upsert-and-links the entity with `epi.id` `\"x\"`"
)]
fn claim_of_partition_2(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    unique_id(&applier)?;
    let placement = owning_value(&applier, "x")?;
    // Its memory is partition 1's own, so only the claim reaches partition 2.
    let memory = owned_by(&placement, 1)?;
    let mut txn = begin(&applier)?.bind(partition(1)?, &placement);
    write(&mut txn, vec![insert_literal(&memory, "kind", "memory")])?;
    txn.operate(upsert("x", Vec::new(), &[&memory]), 0)
        .context("asked")?;
    w.replies = decide_bound(&applier, prepare(txn)?, partition(1)?, &placement)?;
    w.statements = vec![
        statements_of(&applier, "epi.id")?,
        statements_of(&applier, "HAS_ENTITY")?,
    ];
    w.held = held(&applier, &memory, "kind")?;
    Ok(())
}

#[then(
    "the transaction aborts with a cross-partition error naming partitions 1 and 2, not marked retryable, and nothing it wrote is held"
)]
fn claim_cross_partition(w: &mut Steps) -> TestResult {
    assert_eq!(
        outcomes(&w.replies),
        vec![(1, Err(CommitError::cross_partition(1, 2)))]
    );
    assert!(!CommitError::cross_partition(1, 2).is_retryable());
    assert_eq!(w.statements, vec![0, 0]);
    assert!(w.held.is_empty(), "{:?}", w.held);
    Ok(())
}

#[when(
    "a transaction in the partition owning the bucket of `\"w\"` insert-if-absents a node with `epi.id` `\"w\"`, declared unique, and no node holds it"
)]
async fn insert_if_absent_new(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    unique_id(&applier)?;
    let mut txn = begin(&applier)?;
    txn.operate(insert_if_absent("w"), 0).context("asked")?;
    w.replies = commit_concurrently(&applier, vec![prepare(txn)?]).await?;
    let (node, created) = one_node(&w.replies)?;
    w.checks = vec![created == 1];
    w.ids = vec![node_id(&applier, &node)?];
    w.terms = vec![node_terms(&applier, &node, "epi.id")?];
    w.names = vec![node];
    Ok(())
}

#[then(
    "the reply names the created node by its skolem IRI, says it was created, and holds no system ID"
)]
fn named_by_skolem(w: &mut Steps) -> TestResult {
    assert!(w.replies[0].outcome().is_ok(), "{:?}", w.replies);
    let iri = w.names.first().context("the node's IRI")?;
    // A skolem IRI keeps the `/.well-known/genid/` path (`Names::skolem_iri`).
    assert!(iri.contains("/.well-known/genid/"), "{iri}");
    assert_eq!(w.checks, vec![true], "created");
    // The term table holds the IRI alone, and the IRI holds no part of the node's system ID in hexadecimal.
    assert_eq!(w.replies[0].terms().len(), 1);
    let id = w.ids.first().context("the node's ID")?;
    assert!(!iri.contains(&format!("{:032x}", id.to_u128())), "{iri}");
    assert_eq!(
        w.terms,
        vec![vec![Term::from(Literal::new_simple_literal("w"))]]
    );
    Ok(())
}

// Transactions: Commits answer a session token.

#[when("a transaction commits in partition 1 at position 40 with a version of epoch 12")]
fn commits_at_40(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let _version: u64 = setup(&applier, vec![insert("a", "knows", "b")])?;
    // The partition's log ends at position 39 and its latest delivered close is 11.
    let mut driver = Driver::new(&applier, unbounded());
    driver.step = CommitStep::new(unbounded(), applier.quads().lists().applied_ts())
        .with_delivered(epoch(11)?)
        .with_partition(1)
        .with_tail(39);
    let mut txn = begin(&applier)?;
    write(&mut txn, vec![insert("c", "knows", "d")])?;
    driver.submit(1, prepare(txn)?)?;
    driver.sync()?;
    w.replies = driver.replies.clone();
    Ok(())
}

#[then("its reply carries the token (partition 1, position 40, epoch 12)")]
fn token_1_40_12(w: &mut Steps) -> TestResult {
    assert_eq!(outcomes(&w.replies), vec![(1, Ok(version(12, 0)))]);
    let token = w.replies[0].token().context("a token")?;
    assert_eq!(token, &SessionToken::new(1, 40, epoch(12)?));
    Ok(())
}

#[when(
    "a client commits `<a> <knows> <b>` and reads `<a>`'s statements with the commit's token from another connection"
)]
async fn read_with_token(w: &mut Steps) -> TestResult {
    // The partition's half: the commit's token names a log position the store has applied, so a read the token routes
    // there at its watermark holds the commit. Routing the read by the token is compute's (group 9).
    let applier = applier()?;
    let (_storage, log) = open_wal().await?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).context("it opens")?;
    let mut txn = begin(&applier)?;
    write(&mut txn, vec![insert("a", "knows", "b")])?;
    pipe.commit(1, prepare(txn)?, 0).context("appended")?;
    pipe.sync(0).await.context("synced")?;
    w.replies = std::mem::take(pipe.replies_mut());
    let token = w
        .replies
        .first()
        .and_then(Reply::token)
        .context("a token")?;
    w.tokens = vec![token.clone()];
    w.versions = vec![
        Replay::applied_position(applier.quads().store())
            .context("the applied position reads")?
            .get(),
    ];
    w.held = held(&applier, "a", "knows")?;
    Ok(())
}

#[then("the read returns `<a> <knows> <b>`")]
fn read_returns_the_commit(w: &mut Steps) -> TestResult {
    let token = w.tokens.first().context("the token")?;
    // The log's first round, at position 1.
    assert_eq!(token.position(), 1);
    assert!(
        w.versions[0] >= token.position(),
        "the token's round applied"
    );
    assert_eq!(w.held, vec![Term::from(ex("b"))]);
    Ok(())
}

#[when(
    "a session commits in partition 1 at epoch 20, and its next transaction, carrying that token, commits in partition 2 while partition 2's latest delivered close is 17"
)]
fn causally_later(w: &mut Steps) -> TestResult {
    let (store_1, store_2) = (applier()?, applier()?);
    let mut one = driver_at(&store_1, 1, 19)?;
    commit_in(&mut one, &store_1, SESSION, 1, "first")?;
    let token = one
        .replies
        .first()
        .and_then(Reply::token)
        .context("a token")?;
    w.tokens = vec![token.clone()];
    let mut two = driver_at(&store_2, 2, 17)?;
    let mut txn = begin(&store_2)?.with_token(token.clone());
    write(&mut txn, vec![insert("second", "knows", "b")])?;
    two.submit(2, prepare_in(txn, SESSION, 2)?)?;
    w.checks.push(two.appended.is_empty());
    two.close(18)?;
    w.checks.push(two.appended.is_empty());
    two.close(19)?;
    w.checks.push(two.appended.len() == 1);
    two.sync()?;
    w.replies = two.replies.clone();
    Ok(())
}

#[then("partition 2 commits it only after close 19 is delivered, at epoch 20 or later")]
fn not_stamped_earlier(w: &mut Steps) -> TestResult {
    let token = w.tokens.first().context("the token")?;
    assert_eq!((token.partition(), token.epoch()), (1, epoch(20)?));
    // Held through the close of 18; decided once 19 is delivered.
    assert_eq!(w.checks, vec![true, true, true]);
    assert_eq!(outcomes(&w.replies), vec![(2, Ok(version(20, 0)))]);
    let stamped = committed(&w.replies)?;
    assert!(stamped.iter().all(|v| v >> 22 >= 20), "{stamped:?}");
    Ok(())
}

// Recovery replays the log above the store (7.9), by the store's applied log position: a version the scenarios name
// is the round's position in the log.

/// Commits `<s{round}> <knows> <b>` through `pipe` under ticket `round`, appended and not yet synced.
fn append_round(pipe: &mut Pipe<'_>, applier: &Applier, round: u64) -> TestResult {
    let mut txn = begin(applier)?;
    write(&mut txn, vec![insert(&format!("s{round}"), "knows", "b")])?;
    pipe.commit(round, prepare(txn)?, 0).context("appended")?;
    Ok(())
}

/// Replays the log on `storage` into `store`, both reopened after a power loss: the applier, and the replay or
/// recovery's refusal.
async fn restart(
    storage: &SimStorage,
    store: &SimStore,
) -> TestResult<(Applier, Result<Replay, RecoveryError>)> {
    storage.power_loss();
    store.power_loss();
    store.recover();
    let applier = applier_over(store.clone())?;
    let (mut log, _found) = reopen(storage).await?;
    let replayed = Replay::run(&applier, &mut log, Sessions::DEFAULT_CAPACITY).await;
    Ok((applier, replayed))
}

/// Whether `applier`'s store holds the subject of round `round`.
fn holds_round(applier: &Applier, round: u64) -> TestResult<bool> {
    Ok(!held(applier, &format!("s{round}"), "knows")?.is_empty())
}

#[when(
    "rounds 1 to 10 are synced in the log, the store had applied up to round 7, and power is lost"
)]
async fn ten_rounds_seven_applied(w: &mut Steps) -> TestResult {
    let store = sim_store();
    let applier = applier_over(store.clone())?;
    let storage = SimStorage::new(1);
    let (log, _) = reopen(&storage).await?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).context("it opens")?;
    for round in 1..=10 {
        append_round(&mut pipe, &applier, round)?;
        pipe.sync(0).await.context("synced and applied")?;
        if round == 7 {
            applier.quads().store().sync().context("the store syncs")?;
        }
    }
    drop(pipe);
    drop(applier);
    let (recovered, replayed) = restart(&storage, &store).await?;
    let _replayed: Replay = replayed.context("recovered")?;
    w.checks = (1..=10)
        .map(|round| holds_round(&recovered, round))
        .collect::<TestResult<_>>()?;
    Ok(())
}

#[then("after recovery the store holds rounds 1 to 10")]
fn holds_ten(w: &mut Steps) {
    assert_eq!(w.checks, vec![true; 10]);
}

#[when("the store's applied position lies beyond the log's tail")]
async fn store_ahead(w: &mut Steps) -> TestResult {
    let applier = applier()?;
    let (_storage, log) = open_wal().await?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).context("it opens")?;
    for round in 1..=2 {
        append_round(&mut pipe, &applier, round)?;
        pipe.sync(0).await.context("synced and applied")?;
    }
    drop(pipe);
    // Another log, holding one round: the store's position lies beyond its tail.
    let other = applier_over(sim_store())?;
    let (_other_storage, other_log) = open_wal().await?;
    let mut other_pipe: Pipe<'_> =
        Pipeline::new(&other, other_log, unbounded()).context("it opens")?;
    append_round(&mut other_pipe, &other, 1)?;
    other_pipe.sync(0).await.context("synced and applied")?;
    let mut short_log = other_pipe.into_log();
    let refused = Replay::run(&applier, &mut short_log, Sessions::DEFAULT_CAPACITY).await;
    w.recovery = refused.err();
    Ok(())
}

#[then("the node refuses to start, naming the position")]
fn refuses_naming_version(w: &mut Steps) -> TestResult {
    let refused = w.recovery.clone().context("refused")?;
    // The store applied the rounds at positions 1 and 2 of its own log.
    assert_eq!(refused.0, RecoveryErrorEnum::StoreNotInLog { position: 2 });
    assert!(refused.to_string().contains("position 2"), "{refused}");
    Ok(())
}

#[when(
    "a checkpoint at position 40 has trimmed the log, and the store reopened has applied position 30"
)]
async fn behind_the_checkpoint(w: &mut Steps) -> TestResult {
    use ultragraphdb_traits_store::{PersistMode, WriteBatch};
    let applier = applier()?;
    let (_storage, log) = open_wal().await?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).context("it opens")?;
    for round in 1..=40 {
        append_round(&mut pipe, &applier, round)?;
        pipe.sync(0).await.context("synced and applied")?;
    }
    let mut written_log = pipe.into_log();
    written_log
        .trim_through(Position::new(40))
        .await
        .context("the checkpoint is written")?;
    w.versions = vec![written_log.checkpoint().get()];
    // The store reopened as it stood after round 30: its applied position, which every batch records, is 30.
    let reopened = applier_over(sim_store())?;
    let mut batch = WriteBatch::new();
    batch.put_versionless(
        Replay::APPLIED_POSITION_KEY.to_vec(),
        30u64.to_le_bytes().to_vec(),
    );
    reopened
        .quads()
        .store()
        .apply(batch, PersistMode::Buffer)
        .context("the position is written")?;
    w.recovery = Replay::run(&reopened, &mut written_log, Sessions::DEFAULT_CAPACITY)
        .await
        .err();
    w.checks = vec![
        Replay::applied_position(reopened.quads().store())
            .context("the position reads")?
            .get()
            == 30,
    ];
    Ok(())
}

#[then("the node refuses to start, naming 30 and 40, and replays nothing")]
fn refuses_naming_both(w: &mut Steps) -> TestResult {
    assert_eq!(w.versions, vec![40]);
    let refused = w.recovery.clone().context("refused")?;
    assert_eq!(
        refused.0,
        RecoveryErrorEnum::BehindCheckpoint {
            applied: 30,
            checkpoint: 40
        }
    );
    let text = refused.to_string();
    assert!(
        text.contains("position 30") && text.contains("position 40"),
        "{text}"
    );
    assert_eq!(
        w.checks,
        vec![true],
        "replays nothing: the store's position stays 30"
    );
    Ok(())
}

#[when("the node loses power at any step of a round")]
async fn power_at_any_step(w: &mut Steps) -> TestResult {
    // Round 5's steps on the log are its write and its sync; the power is cut at each, the step failing before or after
    // its effect, then the store's power too. A round reaches the store only once the log reports it synced.
    for step in 0..2 {
        for mode in [FaultMode::Before, FaultMode::After] {
            w.checks.push(power_cut_at(step, mode).await?);
        }
    }
    Ok(())
}

/// Commits rounds 1 to 4, cuts the power at `step` of round 5 in `mode`, recovers, and checks that every round the
/// store holds the log holds and that recovery gave the store every round the log holds.
async fn power_cut_at(step: usize, mode: FaultMode) -> TestResult<bool> {
    let store = sim_store();
    let applier = applier_over(store.clone())?;
    let storage = SimStorage::new(step as u64);
    let (log, _) = reopen(&storage).await?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).context("it opens")?;
    for round in 1..=4 {
        append_round(&mut pipe, &applier, round)?;
        pipe.sync(0).await.context("synced and applied")?;
    }
    storage.fail_at(storage.trace().len() + step, mode);
    append_round(&mut pipe, &applier, 5)?;
    // The sync faults at the step; whether it fails or not, recovery is what is checked.
    let _synced: Result<(), PipelineError> = pipe.sync(0).await;
    drop(pipe);
    drop(applier);
    let (recovered, replayed) = restart(&storage, &store).await?;
    let _replayed: Replay = replayed.context("recovered")?;
    let (mut reopened_log, _) = reopen(&storage).await?;
    let logged = reopened_log
        .read(Position::new(1), u32::MAX)
        .await
        .context("read")?
        .len();
    let held: Vec<bool> = (1..=5)
        .map(|round| holds_round(&recovered, round))
        .collect::<TestResult<_>>()?;
    Ok(held.iter().enumerate().all(|(i, h)| *h == (i < logged)))
}

#[then("the store after recovery holds no write of a round the log does not hold")]
fn store_within_the_log(w: &mut Steps) {
    assert_eq!(w.checks.len(), 4);
    assert!(w.checks.iter().all(|held| *held), "{:?}", w.checks);
}

#[when(
    "the simulated storage fails a round's sync with an error, or loses power after a round was acknowledged"
)]
async fn reported_faults(w: &mut Steps) -> TestResult {
    for (seed, fault) in [(1, true), (2, false)] {
        let store = sim_store();
        let applier = applier_over(store.clone())?;
        let storage = SimStorage::new(seed);
        let (log, _) = reopen(&storage).await?;
        let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).context("it opens")?;
        let mut acknowledged = Vec::new();
        for round in 1..=7 {
            if round == 7 && fault {
                // The write succeeds, its sync fails: never acknowledged.
                storage.fail_at(storage.trace().len() + 1, FaultMode::Before);
            }
            append_round(&mut pipe, &applier, round)?;
            if pipe.sync(0).await.is_ok() && !pipe.replies_mut().is_empty() {
                acknowledged.push(round);
            }
            pipe.replies_mut().clear();
        }
        assert_eq!(acknowledged.len(), if fault { 6 } else { 7 });
        drop(pipe);
        drop(applier);
        let (recovered, replayed) = restart(&storage, &store).await?;
        let _replayed: Replay = replayed.context("recovered")?;
        let present = acknowledged
            .iter()
            .map(|round| holds_round(&recovered, *round))
            .collect::<TestResult<Vec<bool>>>()?;
        w.checks.push(present.into_iter().all(|holds| holds));
    }
    Ok(())
}

#[then("after recovery every acknowledged round is present")]
fn acknowledged_present(w: &mut Steps) {
    assert_eq!(w.checks, [true, true]);
}

// Drops are commands: a drop decided in a round through a session (the wire-protocol delta, B2).

/// The store a drop is tried on: `<a> <knows> <b>` in graph `g`, `<c> <knows> <d>` in graph `h`, `<e> <name> "x"` in
/// the default graph after `name` is declared a property.
fn graphs() -> TestResult<Applier> {
    let applier = applier()?;
    let quad = |s: &str, o: &str, g: &str| Change::Insert {
        graph: GraphName::from(ex(g)),
        subject: ex(s).into(),
        predicate: ex("knows"),
        object: ex(o).into(),
    };
    let _version: u64 = setup(
        &applier,
        vec![Change::Declare(Declaration::Predicate(
            PredicateDeclaration::new(ex("name"))
                .with_kind(ultragraphdb_core_mapping::PredicateKind::Property),
        ))],
    )?;
    let _quads: u64 = setup(
        &applier,
        vec![
            quad("a", "b", "g"),
            quad("c", "d", "h"),
            insert_literal("e", "name", "x"),
        ],
    )?;
    Ok(applier)
}

/// The objects of `<subject> <knows>` in graph `<graph>`.
fn held_in(applier: &Applier, graph: &str, subject: &str) -> TestResult<Vec<Term>> {
    let mut out = Vec::new();
    let mut txn = begin(applier)?;
    let (g, s, p) = (GraphName::from(ex(graph)), ex(subject), ex("knows"));
    txn.objects_into(g.as_ref(), s.as_ref().into(), p.as_ref(), 0, &mut out)
        .context("the read runs")?;
    Ok(out)
}

/// A drop of `request` through `session`, request `number`, as the commit step's proposal.
fn drop_of(
    applier: &Applier,
    request: DropRequest<'_>,
    session: u128,
    number: u64,
) -> TestResult<Prepared> {
    Prepared::drop(applier, NS, request, session, number, 0, TxnBuffers::new())
        .context("the drop prepares")
}

/// The drop of graph `<g>`.
fn graph_g(g: &NamedNode) -> DropRequest<'_> {
    DropRequest::Graph(oxrdf::GraphNameRef::NamedNode(g.as_ref()))
}

#[when("a client drops graph `g` of a namespace holding statements in `g` and in `h`")]
async fn drop_graph_g(w: &mut Steps) -> TestResult {
    let applier = graphs()?;
    let (_storage, log) = open_wal().await?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).context("it opens")?;
    let g = ex("g");
    let drop = drop_of(&applier, graph_g(&g), SESSION, 1)?;
    pipe.commit(2, drop, 0).context("appended")?;
    pipe.sync(0).await.context("synced")?;
    w.replies = std::mem::take(pipe.replies_mut());
    w.version = applier.quads().lists().applied_ts();
    w.terms = vec![held_in(&applier, "g", "a")?, held_in(&applier, "h", "c")?];
    Ok(())
}

#[then(
    "the reply carries the drop's commit version, no statement of `g` remains, and every statement of `h` remains"
)]
fn graph_dropped(w: &mut Steps) -> TestResult {
    // The first commit after the set-ups, of epoch 0: (1, 0), which the store applied.
    assert_eq!(outcomes(&w.replies), vec![(2, Ok(version(1, 0)))]);
    assert_eq!(w.version, version(1, 0), "the drop's commit version");
    assert!(w.terms[0].is_empty(), "nothing of g");
    assert_eq!(w.terms[1].len(), 1, "h kept");
    Ok(())
}

#[when("a client drops a namespace's data after declaring `name` a property")]
async fn drop_data(w: &mut Steps) -> TestResult {
    let applier = graphs()?;
    let (_storage, log) = open_wal().await?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).context("it opens")?;
    let drop = drop_of(&applier, DropRequest::Data, SESSION, 1)?;
    pipe.commit(2, drop, 0).context("appended")?;
    pipe.sync(0).await.context("synced")?;
    w.replies = std::mem::take(pipe.replies_mut());
    w.terms = vec![
        held_in(&applier, "g", "a")?,
        held_in(&applier, "h", "c")?,
        held(&applier, "e", "name")?,
    ];
    let catalog = applier.catalog(NS).context("the catalog reads")?;
    w.checks = vec![catalog.predicate(id_of(&applier, "name")?).is_some()];
    Ok(())
}

#[then("no statement of the namespace remains, and Describe still lists `name`")]
fn data_dropped(w: &mut Steps) -> TestResult {
    assert!(w.replies[0].outcome().is_ok(), "{:?}", w.replies);
    assert!(w.terms.iter().all(Vec::is_empty), "{:?}", w.terms);
    assert_eq!(w.checks, vec![true], "the declaration of `name` is kept");
    Ok(())
}

#[when(
    "a Drop's reply is lost, and the client sends it again with the same session and request number"
)]
async fn drop_retried(w: &mut Steps) -> TestResult {
    let applier = graphs()?;
    let (_storage, log) = open_wal().await?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).context("it opens")?;
    let g = ex("g");
    pipe.commit(2, drop_of(&applier, graph_g(&g), SESSION, 1)?, 0)
        .context("appended")?;
    pipe.sync(0).await.context("synced")?;
    let first = pipe.replies_mut().pop().context("the reply, lost")?;
    let applied = applier.quads().lists().applied_ts();
    pipe.commit(3, drop_of(&applier, graph_g(&g), SESSION, 1)?, 0)
        .context("answered")?;
    pipe.sync(0).await.context("synced")?;
    w.replies = vec![first];
    w.replies.append(pipe.replies_mut());
    w.stored = vec![pipe.retry(SESSION, 1).context("the stored reply")?];
    w.versions = vec![applied, applier.quads().lists().applied_ts()];
    Ok(())
}

#[then("the second answer is the stored reply, and the drop is applied once")]
fn drop_once(w: &mut Steps) -> TestResult {
    assert_eq!(w.replies.len(), 2, "{:?}", w.replies);
    assert!(w.replies[0].outcome().is_ok());
    assert_eq!(
        w.replies[0].outcome(),
        w.replies[1].outcome(),
        "the stored reply"
    );
    let stored = w.stored.first().context("the stored reply")?;
    assert_eq!(
        w.replies[0].outcome(),
        &Ok(stored.version()),
        "and on a stream of its own"
    );
    assert_eq!(w.versions[0], w.versions[1], "applied once");
    Ok(())
}

#[when("a transaction reads before a drop of graph `g` commits and then writes into `g`")]
async fn write_into_dropped_graph(w: &mut Steps) -> TestResult {
    let applier = graphs()?;
    let (_storage, log) = open_wal().await?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded()).context("it opens")?;
    let mut txn = begin(&applier)?;
    let _read: Vec<Term> = held_in(&applier, "g", "a")?;
    let g = ex("g");
    pipe.commit(1, drop_of(&applier, graph_g(&g), 0, 0)?, 0)
        .context("appended")?;
    pipe.sync(0).await.context("synced")?;
    write(
        &mut txn,
        vec![Change::Insert {
            graph: GraphName::from(ex("g")),
            subject: ex("late").into(),
            predicate: ex("knows"),
            object: ex("b").into(),
        }],
    )?;
    pipe.commit(2, prepare(txn)?, 0).context("appended")?;
    pipe.sync(0).await.context("synced")?;
    w.replies = std::mem::take(pipe.replies_mut());
    Ok(())
}

#[then("its commit is refused with a conflict of kind Drop")]
fn drop_conflict(w: &mut Steps) -> TestResult {
    assert_eq!(
        reply_to(&w.replies, 2)?.outcome(),
        &Err(CommitError::conflict(ConflictKind::Drop)),
        "{:?}",
        w.replies
    );
    Ok(())
}

// T1 creates `<gone>` and `<made>`; T2 takes `<gone>` from it. A probe writing both while T1 is open takes T1's IDs,
// so they can be compared, and is released unapplied. A later writer of `<made>`, which only T1 and the probe held,
// shows whether T1's hold was let go.
#[when(
    "transaction T1 creates `<gone>`, transaction T2 writes `<gone>` while T1 is open, T1 aborts and T2 commits, and the store is discarded and the log replayed into an empty store"
)]
async fn released_entry_replayed(w: &mut Steps) -> TestResult {
    let original = applier()?;
    let (_storage, log) = open_wal().await?;
    let mut pipe: Pipe<'_> = Pipeline::new(&original, log, unbounded())?;
    let mut t1 = begin(&original)?;
    write(&mut t1, vec![insert("gone", "made", "a")])?;
    let mut t2 = begin(&original)?;
    write(&mut t2, vec![insert_literal("gone", "name", "G")])?;
    let mut probe = begin(&original)?;
    write(&mut probe, vec![insert("made", "knows", "gone")])?;
    let probe = prepare(probe)?;
    let (t1_gone, t1_made) = (id_in(&probe, "gone")?, id_in(&probe, "made")?);
    let _probe_buffers = probe.release(&original);
    let _t1_buffers = t1.abort();
    let t2 = prepare(t2)?;
    let gone_iri = ex("gone");
    let carried = t2
        .dictionary()
        .any(|(_, value)| value == gone_iri.as_str().as_bytes());
    let t2_gone = id_in(&t2, "gone")?;
    pipe.commit(1, t2, 0)?;
    pipe.sync(0).await?;
    let committed = reply_to(pipe.replies_mut(), 1)?.outcome().clone();
    let mut later = begin(&original)?;
    write(&mut later, vec![insert("made", "knows", "a")])?;
    let later = prepare(later)?;
    let later_made = id_in(&later, "made")?;
    let _later_buffers = later.release(&original);
    let stored_gone = id_of(&original, "gone")?;
    let mut written = pipe.into_log();
    let replica = applier_over(sim_store())?;
    let _replayed = replay_into(&replica, &mut written).await?;
    w.ids = vec![t2_gone, stored_gone, id_of(&replica, "gone")?];
    w.expected_ids = vec![t1_gone];
    w.checks = vec![
        committed.is_ok(),
        carried,
        later_made != t1_made,
        original
            .quads()
            .id_of(NS, ex("made").as_str())
            .context("the dictionary reads")?
            .is_none(),
        replica
            .quads()
            .id_of(NS, ex("made").as_str())
            .context("the dictionary reads")?
            .is_none(),
    ];
    w.held = held(&replica, "gone", "made")?;
    w.held.extend(held(&original, "gone", "made")?);
    w.terms = vec![held(&replica, "gone", "name")?];
    Ok(())
}

#[then(
    "T2's record carries `<gone>`'s entry, replay resolves `<gone>` to the ID T2 committed, and nothing T1 created is held"
)]
fn released_entry_absent(w: &mut Steps) -> TestResult {
    let [
        committed,
        carried,
        made_fresh,
        original_no_made,
        replica_no_made,
    ] = w.checks[..]
    else {
        return Err(TestError::new(format!("five checks: {:?}", w.checks)));
    };
    assert!(committed, "T2 commits");
    assert!(carried, "T2's round carries `<gone>`'s dictionary entry");
    let t1_gone = *w.expected_ids.first().context("T1's ID for `<gone>`")?;
    assert_eq!(
        w.ids,
        vec![t1_gone, t1_gone, t1_gone],
        "T2 commits the ID it took from T1, the store holds it, and replay of the log alone resolves `<gone>` to it"
    );
    assert!(
        made_fresh,
        "T1's `<made>` was released: a later writer is given another ID"
    );
    assert!(original_no_made, "the store holds no entry for `<made>`");
    assert!(replica_no_made, "replay writes no entry for `<made>`");
    assert!(w.held.is_empty(), "T1's statement is held: {:?}", w.held);
    assert_eq!(
        w.terms,
        vec![vec![Term::from(Literal::new_simple_literal("G"))]],
        "T2's statement is replayed"
    );
    Ok(())
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}

/// Replays `log` into `applier`'s store from its applied position.
async fn replay_into(applier: &Applier, log: &mut Log) -> Result<Replay, RecoveryError> {
    Replay::run(applier, log, Sessions::DEFAULT_CAPACITY).await
}

/// The applied log position `applier`'s store records.
fn applied_position(applier: &Applier) -> TestResult<u64> {
    Ok(Replay::applied_position(applier.quads().store())
        .context("the position reads")?
        .get())
}

// The store before the two rounds is rebuilt by replaying the log's first round into an empty store, so "restored to
// its state before them" and "their records replayed onto it" are the rounds of one replay.
#[when(
    "a store holds `p`'s keys and statements a delete by pattern will remove, a round holding a drop of `p` and the next round holding that delete by pattern and a commit of session `s`'s request 7 are applied, the store is restored to its state before them, and their records are replayed onto it"
)]
async fn replay_drop_pattern_session(w: &mut Steps) -> TestResult {
    let original = applier()?;
    let (_storage, log) = open_wal().await?;
    let mut pipe: Pipe<'_> = Pipeline::new(&original, log, unbounded())?;
    let mut before = begin(&original)?;
    write(
        &mut before,
        vec![
            insert_literal("a", "p", "1"),
            insert("b", "knows", "c"),
            insert("b", "knows", "d"),
        ],
    )?;
    pipe.commit(1, prepare(before)?, 0)?;
    pipe.sync(0).await?;
    let drop = Prepared::drop(
        &original,
        NS,
        DropRequest::Predicate(ex("p").as_ref()),
        0,
        0,
        0,
        TxnBuffers::new(),
    )?;
    pipe.commit(2, drop, 0)?;
    let mut pattern = begin(&original)?;
    write(
        &mut pattern,
        vec![Change::DeletePattern {
            graph: GraphName::DefaultGraph,
            subject: ex("b").into(),
            predicate: ex("knows"),
        }],
    )?;
    pipe.commit(3, prepare(pattern)?, 0)?;
    let mut session = begin(&original)?;
    write(&mut session, vec![insert("s", "knows", "b")])?;
    pipe.commit(4, prepare_in(session, SESSION, 7)?, 0)?;
    // The drop closed its round on arrival; the delete and request 7 wait for its sync and close the next round.
    pipe.sync(0).await?;
    pipe.sync(0).await?;
    let answers: Vec<(u64, Result<u64, CommitError>)> = pipe
        .replies_mut()
        .iter()
        .map(|reply| (reply.ticket(), reply.outcome().clone()))
        .collect();
    let committed = answers
        .iter()
        .find(|(ticket, _)| *ticket == 4)
        .map(|(_, outcome)| outcome.clone())
        .context(&format!("request 7 answered: {answers:?}"))??;
    let mut written = pipe.into_log();
    let replica = applier_over(sim_store())?;
    let replayed = replay_into(&replica, &mut written).await?;
    w.checks = vec![replayed.rounds() == 3];
    w.held = held(&replica, "a", "p")?;
    w.held.extend(held(&replica, "b", "knows")?);
    let mut again: Pipe<'_> = Pipeline::new(&replica, written, unbounded())?;
    let stored = again.retry(SESSION, 7)?.version();
    let mut retry = begin(&replica)?;
    write(&mut retry, vec![insert("s", "knows", "b")])?;
    again.commit(5, prepare_in(retry, SESSION, 7)?, 0)?;
    let answered = again
        .replies_mut()
        .pop()
        .context("the retry is answered")?
        .outcome()
        .clone()?;
    w.versions = vec![committed, stored, answered];
    Ok(())
}

#[then(
    "no key of `p` and none of the removed statements is held, and a retry of `s`'s request 7 returns its stored reply without committing again"
)]
fn drops_and_sessions_rebuilt(w: &mut Steps) -> TestResult {
    assert_eq!(w.checks, vec![true], "the log held the three rounds");
    assert!(w.held.is_empty(), "held after replay: {:?}", w.held);
    let committed = *w.versions.first().context("the commit's version")?;
    assert_eq!(
        w.versions,
        vec![committed, committed, committed],
        "the retry answers request 7's stored version, committing nothing new"
    );
    Ok(())
}

#[when("a record names a derivation version above the reader's")]
async fn record_with_unknown_derivation(w: &mut Steps) -> TestResult {
    let original = applier()?;
    let (_storage, log) = open_wal().await?;
    let mut pipe: Pipe<'_> = Pipeline::new(&original, log, unbounded())?;
    let mut txn = begin(&original)?;
    write(&mut txn, vec![insert("a", "knows", "b")])?;
    pipe.commit(1, prepare(txn)?, 0)?;
    pipe.sync(0).await?;
    let mut written = pipe.into_log();
    let (_, parent) = written.tail();
    let derivation = std::num::NonZeroU32::new(2).context("2")?;
    let body = {
        use ultragraphdb_protocol_wire::{BodyBuilder, RoundBody};
        RoundBody::new(2, parent, 0, version(1, 2), derivation)
            .body()
            .to_vec()
    };
    let _ticket = written
        .append(ultragraphdb_traits_log::RoundRef::new(
            Position::new(2),
            parent,
            version(1, 2),
            0,
            derivation,
            &body,
        ))
        .context("the log takes the round")?;
    let _synced: Position = written.sync().await.context("synced")?;
    let replica = applier_over(sim_store())?;
    w.recovery = replay_into(&replica, &mut written).await.err();
    w.versions = vec![applied_position(&replica)?];
    Ok(())
}

#[then("replay fails with an error naming the version, and applies nothing of the record")]
fn unknown_derivation_refused(w: &mut Steps) -> TestResult {
    let refused = w.recovery.clone().context("replay refused")?;
    assert_eq!(
        refused.0,
        RecoveryErrorEnum::Derivation {
            position: 2,
            derivation: 2
        }
    );
    assert!(refused.to_string().contains("derivation 2"), "{refused}");
    assert_eq!(w.versions, vec![1], "round 1 replayed, nothing of round 2");
    Ok(())
}
