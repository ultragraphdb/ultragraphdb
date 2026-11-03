/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The commit step's epochs (commit-rounds, "Commit versions are built from the epoch"; sequencer, "Partitions learn
//! each closed epoch and stamp the open one"): versions from the open epoch, closes delivered as inputs, the close
//! request of a full epoch and the busy answer once it goes unanswered, the epoch a partition has closed, session
//! tokens, the catalog version in effect, registrations a home accepted, and the session table across partitions. Every
//! version below is written from the layout `epoch << 22 | sequence` ([`version`]).

use ultragraphdb_cluster_sequencer::CatalogVersion;
use ultragraphdb_core_ids::{Epoch, Namespace, SessionToken};
use ultragraphdb_protocol_wire::{ConflictKind, RecordKind, RoundHeader, RoundView};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transactions_commit::{
    CloseRequest, CommitError, CommitRequest, CommitStep, Input, Reply, RoundBounds, StepOutput,
};

use crate::utils::fixture::{close, version};
use crate::utils::proposal_double::{
    Reads, TestProposal, Writes, proposal, proposal_at_catalog, proposal_in, registration,
};

/// 2^22 − 2, the last sequence of an epoch, by hand: 4,194,304 − 2.
const LAST: u64 = 4_194_302;

type Step = CommitStep<TestProposal>;

fn unbounded() -> RoundBounds {
    RoundBounds::new(u32::MAX, u32::MAX)
}

/// A step after version `previous` whose latest delivered close is `delivered`.
fn step(previous: u64, delivered: u64) -> TestResult<Step> {
    Ok(CommitStep::new(unbounded(), previous)
        .with_delivered(Epoch::new(delivered).context("epoch")?))
}

/// A proposal writing the point `k{n}`, reading nothing.
// Read at a version above every version the tests stamp, so no step's history floor makes it too old.
fn writer(n: u64) -> TestResult<TestProposal> {
    proposal(
        u64::MAX >> 1,
        &Reads::default(),
        &Writes::point(format!("k{n}").as_bytes()),
    )
}

/// Steps `input`, returning the output.
fn run(step: &mut Step, input: Input<TestProposal>, now_us: u64) -> StepOutput<TestProposal> {
    let mut out = StepOutput::new();
    step.step(input, now_us, &mut out);
    out
}

fn commit(step: &mut Step, ticket: u64, p: TestProposal, now_us: u64) -> StepOutput<TestProposal> {
    run(step, Input::Commit(CommitRequest::new(ticket, p)), now_us)
}

fn deliver(step: &mut Step, epoch: u64) -> TestResult<StepOutput<TestProposal>> {
    Ok(run(
        step,
        Input::Close(close(epoch, Vec::new(), Vec::new())?),
        0,
    ))
}

fn outcomes(out: &StepOutput<TestProposal>) -> Vec<(u64, Result<u64, CommitError>)> {
    out.replies()
        .iter()
        .map(|r| (r.ticket(), r.outcome().clone()))
        .collect()
}

#[test]
fn test_the_first_commit_after_close_9_takes_epoch_10_sequence_0() -> TestResult {
    let mut step = step(0, 9)?;
    let mut out = commit(&mut step, 1, writer(1)?, 0);
    let (record, _) = out.append().context("a round closes")?;
    assert_eq!(record.first_version(), version(10, 0));
    out = run(&mut step, Input::Synced, 0);
    assert_eq!(outcomes(&out), vec![(1, Ok(version(10, 0)))]);
    assert_eq!(step.delivered(), Epoch::new(9)?);
    assert_eq!(step.open(), Some(Epoch::new(10)?));
    Ok(())
}

#[test]
fn test_three_commits_of_one_epoch_take_sequences_0_2_and_4() -> TestResult {
    let mut step = step(0, 9)?;
    let _first = commit(&mut step, 1, writer(1)?, 0);
    let _second = commit(&mut step, 2, writer(2)?, 0);
    let _third = commit(&mut step, 3, writer(3)?, 0);
    let mut replies = outcomes(&run(&mut step, Input::Synced, 0));
    replies.extend(outcomes(&run(&mut step, Input::Synced, 0)));
    assert_eq!(
        replies,
        vec![
            (1, Ok(version(10, 0))),
            (2, Ok(version(10, 2))),
            (3, Ok(version(10, 4)))
        ]
    );
    Ok(())
}

#[test]
fn test_a_step_resumes_after_its_previous_version_in_the_open_epoch() -> TestResult {
    // Opened after version (10, 4), its latest delivered close 9 by default: the open epoch is 10, sequence 6 next.
    let mut step = CommitStep::new(unbounded(), version(10, 4));
    assert_eq!(step.delivered(), Epoch::new(9)?);
    let out = commit(&mut step, 1, writer(1)?, 0);
    assert_eq!(
        out.append().map(|(r, _)| r.first_version()),
        Some(version(10, 6))
    );
    Ok(())
}

#[test]
fn test_a_full_epoch_asks_for_a_close_holds_its_commits_and_stamps_the_next_epoch() -> TestResult {
    let mut step = step(version(10, LAST - 2), 9)?;
    // The last sequence is taken; the round closes with it.
    let mut out = commit(&mut step, 1, writer(1)?, 0);
    assert_eq!(
        out.append().map(|(r, _)| r.first_version()),
        Some(version(10, LAST))
    );
    assert_eq!(out.close_request(), None, "not full until the next asks");
    let _synced = run(&mut step, Input::Synced, 0);
    // The next finds the epoch full: it asks for the close of 10 and holds the commit.
    out = commit(&mut step, 2, writer(2)?, 7);
    assert_eq!(out.append(), None);
    assert_eq!(out.close_request(), Some(Epoch::new(10)?));
    assert!(out.replies().is_empty());
    assert_eq!(step.held(), 1);
    // A second held commit asks nothing again.
    out = commit(&mut step, 3, writer(3)?, 8);
    assert_eq!(out.close_request(), None);
    assert_eq!(step.held(), 2);
    // The close of 10 opens epoch 11 at sequence 0, and the round closes over both.
    out = deliver(&mut step, 10)?;
    assert_eq!(
        out.append()
            .map(|(r, _)| (r.first_version(), r.transactions())),
        Some((version(11, 0), 2))
    );
    assert_eq!(step.held(), 0);
    assert_eq!(
        outcomes(&run(&mut step, Input::Synced, 0)),
        vec![(2, Ok(version(11, 0))), (3, Ok(version(11, 2)))]
    );
    Ok(())
}

#[test]
fn test_a_round_filling_the_epoch_takes_what_fits_and_holds_the_rest() -> TestResult {
    let mut step = step(version(10, LAST - 4), 9)?;
    let _first = commit(&mut step, 1, writer(1)?, 0);
    for ticket in 2..=4 {
        let _waiting = commit(&mut step, ticket, writer(ticket)?, 0);
    }
    // The round in flight took (10, LAST − 2); of the three waiting, one takes LAST and two are held.
    let out = run(&mut step, Input::Synced, 0);
    assert_eq!(
        out.append()
            .map(|(r, _)| (r.first_version(), r.transactions())),
        Some((version(10, LAST), 1))
    );
    assert_eq!(out.close_request(), Some(Epoch::new(10)?));
    assert_eq!(step.held(), 2);
    Ok(())
}

#[test]
fn test_a_close_request_unanswered_for_close_request_ms_refuses_held_and_new_commits_busy()
-> TestResult {
    let mut step = CommitStep::new(unbounded(), version(10, LAST))
        .with_delivered(Epoch::new(9)?)
        .with_close_request(CloseRequest::new(500, 250).context("the setting")?);
    let asked = 1_000;
    let mut out = commit(&mut step, 1, writer(1)?, asked);
    assert_eq!(out.close_request(), Some(Epoch::new(10)?));
    // 499,999 µs later the sequencer still counts as reachable.
    out = run(&mut step, Input::Tick, asked + 499_999);
    assert!(out.replies().is_empty());
    assert_eq!(step.held(), 1);
    // 500 ms after the request, every held commit is refused busy, retryable, and released, writing nothing.
    out = run(&mut step, Input::Tick, asked + 500_000);
    assert_eq!(out.replies().len(), 1);
    let refusal = out
        .replies()
        .first()
        .and_then(|r| r.outcome().clone().err())
        .context("refused")?;
    assert_eq!(refusal.code(), ultragraphdb_protocol_wire::ErrorCode::Busy);
    assert!(refusal.is_retryable());
    assert_eq!(out.released().len(), 1);
    assert_eq!(out.append(), None);
    assert_eq!(step.held(), 0);
    // A commit arriving meanwhile is refused at once.
    out = commit(&mut step, 2, writer(2)?, asked + 600_000);
    assert!(matches!(outcomes(&out).as_slice(), [(2, Err(e))] if e.is_retryable()));
    assert_eq!(out.released().len(), 1);
    // Once the close of 10 arrives, commits flow again in epoch 11.
    let _delivered = deliver(&mut step, 10)?;
    out = commit(&mut step, 3, writer(3)?, asked + 700_000);
    assert_eq!(
        out.append().map(|(r, _)| r.first_version()),
        Some(version(11, 0))
    );
    Ok(())
}

#[test]
fn test_a_close_delivered_while_a_round_syncs_opens_the_next_round_in_the_next_epoch() -> TestResult
{
    let mut step = step(0, 9)?;
    let _first = commit(&mut step, 1, writer(1)?, 0);
    let mut out = deliver(&mut step, 10)?;
    assert_eq!(out.append(), None, "a round is in flight");
    let _second = commit(&mut step, 2, writer(2)?, 0);
    out = run(&mut step, Input::Synced, 0);
    assert_eq!(outcomes(&out), vec![(1, Ok(version(10, 0)))]);
    assert_eq!(
        out.append().map(|(r, _)| r.first_version()),
        Some(version(11, 0))
    );
    Ok(())
}

#[test]
fn test_a_partition_has_closed_an_epoch_once_its_rounds_at_or_below_it_are_applied() -> TestResult {
    let mut step = step(0, 7)?;
    assert_eq!(step.closed(), Epoch::new(7)?);
    let _commit = commit(&mut step, 1, writer(1)?, 0);
    // A round of epoch 8 in flight; the close of 8 arrives: 8 is not closed until the round is applied.
    let _delivered = deliver(&mut step, 8)?;
    assert_eq!(step.delivered(), Epoch::new(8)?);
    assert_eq!(step.closed(), Epoch::new(7)?);
    let _synced = run(&mut step, Input::Synced, 0);
    assert_eq!(step.closed(), Epoch::new(8)?);
    // A round of the open epoch in flight leaves the delivered close closed.
    let _next = commit(&mut step, 2, writer(2)?, 0);
    assert_eq!(step.closed(), Epoch::new(8)?);
    Ok(())
}

#[test]
fn test_closes_missed_are_taken_in_order_and_one_past_a_gap_stops_the_step() -> TestResult {
    let mut caught_up = step(0, 3)?;
    for epoch in 4..=9 {
        let delivered = deliver(&mut caught_up, epoch)?;
        assert!(delivered.replies().is_empty());
    }
    assert!(!caught_up.is_stopped());
    let mut out = commit(&mut caught_up, 1, writer(1)?, 0);
    assert_eq!(
        out.append().map(|(r, _)| r.first_version()),
        Some(version(10, 0))
    );
    // A close delivered already changes nothing.
    let _repeat = deliver(&mut caught_up, 9)?;
    assert!(!caught_up.is_stopped());
    // A close past a gap stops the step: what waits is answered unavailable.
    let mut gapped = step(version(4, LAST), 3)?;
    let _held = commit(&mut gapped, 1, writer(1)?, 0);
    out = deliver(&mut gapped, 5)?;
    assert!(gapped.is_stopped());
    assert_eq!(gapped.fault(), Some(&CommitError::close_out_of_order(3, 5)));
    assert_eq!(outcomes(&out), vec![(1, Err(CommitError::unavailable()))]);
    assert_eq!(out.released().len(), 1);
    out = commit(&mut gapped, 2, writer(2)?, 0);
    assert_eq!(outcomes(&out), vec![(2, Err(CommitError::unavailable()))]);
    Ok(())
}

fn token(partition: u32, position: u64, epoch: u64) -> TestResult<SessionToken> {
    Ok(SessionToken::new(partition, position, Epoch::new(epoch)?))
}

#[test]
fn test_a_commit_carrying_a_token_of_epoch_t_waits_for_the_close_of_t_minus_1() -> TestResult {
    let mut step = step(0, 17)?;
    let carried = writer(1)?.with_token(token(1, 40, 20)?);
    let mut out = commit(&mut step, 1, carried, 0);
    assert_eq!(out.append(), None);
    assert_eq!(step.held(), 1);
    out = deliver(&mut step, 18)?;
    assert_eq!(out.append(), None);
    out = deliver(&mut step, 19)?;
    assert_eq!(
        out.append().map(|(r, _)| r.first_version()),
        Some(version(20, 0))
    );
    assert_eq!(step.held(), 0);
    // A token whose epoch is open, or below it, holds nothing.
    out = run(&mut step, Input::Synced, 0);
    assert_eq!(outcomes(&out), vec![(1, Ok(version(20, 0)))]);
    out = commit(&mut step, 2, writer(2)?.with_token(token(2, 1, 20)?), 0);
    assert!(out.append().is_some());
    let _synced = run(&mut step, Input::Synced, 0);
    out = commit(&mut step, 3, writer(3)?.with_token(token(2, 1, 3)?), 0);
    assert!(out.append().is_some());
    Ok(())
}

#[test]
fn test_a_commits_reply_carries_its_session_token() -> TestResult {
    let mut step = step(0, 11)?.with_partition(1).with_tail(39);
    let _commit = commit(&mut step, 1, writer(1)?, 0);
    let mut out = run(&mut step, Input::Synced, 0);
    let reply = out.replies().first().context("a reply")?;
    assert_eq!(reply.outcome(), &Ok(version(12, 0)));
    assert_eq!(reply.token(), Some(&token(1, 40, 12)?));
    assert_eq!(step.tail(), 40);
    // A refusal carries none.
    let mut refusing = step;
    let _round = commit(
        &mut refusing,
        2,
        proposal(0, &Reads::list(b"k1"), &Writes::default())?,
        0,
    );
    out = run(&mut refusing, Input::Synced, 0);
    let refused = out.replies().first().context("a reply")?;
    assert!(refused.outcome().is_err());
    assert_eq!(refused.token(), None);
    Ok(())
}

#[test]
fn test_a_transaction_executed_against_another_catalog_version_conflicts_naming_the_catalog()
-> TestResult {
    let ns = Namespace::new(0);
    let mut step = step(0, 38)?.with_catalog(&[CatalogVersion::new(ns, 2)]);
    assert_eq!(step.catalog_version(0), 2);
    assert_eq!(
        step.catalog_version(1),
        0,
        "a namespace not named is at version 0"
    );
    let current = proposal_at_catalog(2, 0, &Reads::default(), &Writes::point(b"a"))?;
    let stale = proposal_at_catalog(1, 0, &Reads::default(), &Writes::point(b"b"))?;
    let _first = commit(&mut step, 1, current, 0);
    let _second = commit(&mut step, 2, stale, 0);
    let mut replies = outcomes(&run(&mut step, Input::Synced, 0));
    replies.extend(outcomes(&run(&mut step, Input::Synced, 0)));
    assert_eq!(
        replies,
        vec![
            (1, Ok(version(39, 0))),
            (2, Err(CommitError::conflict(ConflictKind::Catalog)))
        ]
    );
    // The close of 39 announces version 3, in effect at epoch 40.
    let _delivered = run(
        &mut step,
        Input::Close(close(39, vec![CatalogVersion::new(ns, 3)], Vec::new())?),
        0,
    );
    assert_eq!(step.catalog_version(0), 3);
    let _old = commit(
        &mut step,
        3,
        proposal_at_catalog(2, 0, &Reads::default(), &Writes::point(b"c"))?,
        0,
    );
    assert_eq!(
        outcomes(&run(&mut step, Input::Synced, 0)),
        vec![(3, Err(CommitError::conflict(ConflictKind::Catalog)))]
    );
    let _new = commit(
        &mut step,
        4,
        proposal_at_catalog(3, 0, &Reads::default(), &Writes::point(b"d"))?,
        0,
    );
    assert_eq!(
        outcomes(&run(&mut step, Input::Synced, 0)),
        vec![(4, Ok(version(40, 0)))]
    );
    Ok(())
}

#[test]
fn test_a_catalog_conflict_comes_after_the_reads_and_before_a_drop() -> TestResult {
    let mut step = step(0, 9)?.with_catalog(&[CatalogVersion::new(Namespace::new(0), 2)]);
    // A list written above the read version, and a range dropped there.
    let _writes = commit(
        &mut step,
        1,
        proposal_at_catalog(2, 0, &Reads::default(), &Writes::point(b"l"))?,
        0,
    );
    let _synced = run(&mut step, Input::Synced, 0);
    let _drop = commit(
        &mut step,
        2,
        proposal_at_catalog(2, 0, &Reads::default(), &Writes::range(b"r", Some(b"s")))?,
        0,
    );
    let _dropped = run(&mut step, Input::Synced, 0);
    let list_and_stale = proposal_at_catalog(1, 0, &Reads::list(b"l"), &Writes::default())?;
    let dropped_and_stale = proposal_at_catalog(1, 0, &Reads::list(b"r1"), &Writes::default())?;
    let _a = commit(&mut step, 3, list_and_stale, 0);
    let _b = commit(&mut step, 4, dropped_and_stale, 0);
    let mut replies = outcomes(&run(&mut step, Input::Synced, 0));
    replies.extend(outcomes(&run(&mut step, Input::Synced, 0)));
    assert_eq!(
        replies,
        vec![
            (3, Err(CommitError::conflict(ConflictKind::List))),
            (4, Err(CommitError::conflict(ConflictKind::Catalog)))
        ]
    );
    Ok(())
}

#[test]
fn test_a_registration_a_home_accepted_takes_a_version_of_its_own() -> TestResult {
    let mut step = step(0, 9)?.with_partition(2).with_tail(4);
    let record = registration(7, 3, 1)?;
    let mut out = run(
        &mut step,
        Input::Registration {
            ticket: 5,
            record: record.clone(),
        },
        0,
    );
    let (round, body) = out.append().context("a round closes")?;
    assert_eq!(
        (
            round.first_version(),
            round.transactions(),
            round.committed()
        ),
        (version(10, 0), 1, 1)
    );
    // The round logs the registration's record unchanged, kind 2.
    let header = RoundHeader::new(5, 0, 0, round.first_version(), 1, std::num::NonZeroU32::MIN);
    let view = RoundView::from_body(header, body).context("the round reads")?;
    let logged = view.records().first().context("a record")?;
    assert_eq!(logged.kind(), RecordKind::Registration);
    assert_eq!(logged.bytes(), record.as_slice());
    out = run(&mut step, Input::Synced, 0);
    let reply = out.replies().first().context("a reply")?;
    assert_eq!(reply.outcome(), &Ok(version(10, 0)));
    // Its token's epoch is the home's open epoch: the registrant waits for the close before it, 9.
    assert_eq!(reply.token(), Some(&token(2, 5, 10)?));
    Ok(())
}

#[test]
fn test_a_registration_whose_record_does_not_read_is_refused() -> TestResult {
    let mut step = step(0, 9)?;
    let out = run(
        &mut step,
        Input::Registration {
            ticket: 5,
            record: vec![1, 2, 3],
        },
        0,
    );
    let _synced = run(&mut step, Input::Synced, 0);
    let mut replies = outcomes(&out);
    replies.extend(outcomes(&_synced));
    assert!(
        matches!(replies.as_slice(), [(5, Err(e))] if e.code() == ultragraphdb_protocol_wire::ErrorCode::Protocol),
        "{replies:?}"
    );
    Ok(())
}

#[test]
fn test_a_session_commits_through_two_partitions_with_gaps() -> TestResult {
    let session = 0xAB_u128 << 100;
    let mut one = step(0, 9)?.with_partition(1);
    let mut two = step(0, 9)?.with_partition(2);
    let p = |request| proposal_in(session, request, 0, &Reads::default(), &Writes::point(b"x"));
    let _r1 = commit(&mut one, 1, p(1)?, 0);
    let first = run(&mut one, Input::Synced, 0);
    assert_eq!(outcomes(&first), vec![(1, Ok(version(10, 0)))]);
    let _r2 = commit(&mut two, 2, p(2)?, 0);
    let _synced = run(&mut two, Input::Synced, 0);
    // Partition 1 takes request 3 after its last, 1.
    let _r3 = commit(&mut one, 3, p(3)?, 0);
    let third = run(&mut one, Input::Synced, 0);
    let reply: Reply = third.replies().first().cloned().context("a reply")?;
    assert_eq!(reply.outcome(), &Ok(version(10, 2)));
    // The retry on a new connection, with no transaction open, receives request 3's stored reply.
    let stored = one.retry(session, 3).context("the stored reply")?;
    assert_eq!(stored.version(), version(10, 2));
    assert_eq!(Some(stored.token()), reply.token());
    assert_eq!(one.sessions().last(session), Some((3, version(10, 2))));
    // Resent as a commit, it commits nothing again and answers the same.
    let mut out = commit(&mut one, 4, p(3)?, 0);
    assert_eq!(out.append(), None, "nothing to log");
    assert_eq!(outcomes(&out), vec![(4, Ok(version(10, 2)))]);
    assert_eq!(out.replies().first().and_then(Reply::token), reply.token());
    assert_eq!(out.released().len(), 1);
    // Below the last is out of order, naming the last.
    out = commit(&mut one, 5, p(2)?, 0);
    let refused = run(&mut one, Input::Synced, 0);
    let mut replies = outcomes(&out);
    replies.extend(outcomes(&refused));
    assert_eq!(
        replies,
        vec![(5, Err(CommitError::request_out_of_order(3)))]
    );
    Ok(())
}

#[test]
fn test_a_partition_purges_a_session_only_with_the_close_carrying_its_eviction() -> TestResult {
    let session = 77;
    let mut step = step(0, 9)?;
    let _r = commit(
        &mut step,
        1,
        proposal_in(session, 1, 0, &Reads::default(), &Writes::point(b"x"))?,
        0,
    );
    let _synced = run(&mut step, Input::Synced, 0);
    assert_eq!(step.sessions().last(session), Some((1, version(10, 0))));
    let _plain = deliver(&mut step, 10)?;
    assert_eq!(
        step.sessions().last(session),
        Some((1, version(10, 0))),
        "kept"
    );
    let _evicting = run(
        &mut step,
        Input::Close(close(11, Vec::new(), vec![session])?),
        0,
    );
    assert_eq!(step.sessions().last(session), None, "purged");
    Ok(())
}

#[test]
fn test_a_retry_of_a_request_never_committed_is_refused_naming_it() -> TestResult {
    let session = 9;
    let mut step = step(0, 9)?;
    let _r = commit(
        &mut step,
        1,
        proposal_in(session, 7, 0, &Reads::default(), &Writes::point(b"x"))?,
        0,
    );
    let _synced = run(&mut step, Input::Synced, 0);
    assert_eq!(step.retry(session, 8), Err(CommitError::not_committed(8)));
    assert_eq!(CommitError::not_committed(8).detail(), 8);
    Ok(())
}

#[test]
fn test_a_retry_while_its_session_has_a_commit_in_the_step_is_busy() -> TestResult {
    let session = 9;
    let mut step = step(version(10, LAST), 9)?;
    // Held for the close: in the step, not yet decided.
    let _held = commit(
        &mut step,
        1,
        proposal_in(session, 1, 0, &Reads::default(), &Writes::point(b"x"))?,
        0,
    );
    assert_eq!(
        step.retry(session, 1),
        Err(CommitError::session_busy(session))
    );
    Ok(())
}
