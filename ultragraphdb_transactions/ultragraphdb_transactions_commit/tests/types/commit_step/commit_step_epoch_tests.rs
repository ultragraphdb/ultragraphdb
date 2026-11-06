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
use ultragraphdb_core_ids::{Epoch, SessionToken, Tenant};
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
    let through = proposal_in(
        1,
        1,
        u64::MAX >> 1,
        &Reads::default(),
        &Writes::point(b"k1"),
    )?;
    let _commit = commit(&mut step, 1, through, 0);
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
    assert_eq!(
        refused.outcome(),
        &Err(CommitError::conflict(ConflictKind::List))
    );
    assert_eq!(refused.token(), None);
    Ok(())
}

#[test]
fn test_a_transaction_executed_against_another_catalog_version_conflicts_naming_the_catalog()
-> TestResult {
    let tenant = Tenant::new(0);
    let mut step = step(0, 38)?.with_catalog(&[CatalogVersion::new(tenant, 2)]);
    assert_eq!(step.catalog_version(0), 2);
    assert_eq!(
        step.catalog_version(1),
        0,
        "a tenant not named is at version 0"
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
        Input::Close(close(39, vec![CatalogVersion::new(tenant, 3)], Vec::new())?),
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
    let mut step = step(0, 9)?.with_catalog(&[CatalogVersion::new(Tenant::new(0), 2)]);
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
            dictionary: Vec::new(),
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
            dictionary: Vec::new(),
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
    // Resent as a one-frame write, it commits nothing again and answers the same.
    let mut out = commit(&mut one, 4, p(3)?.with_one_frame(), 0);
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

/// A registration whose record reads as a transaction's or a build's is refused malformed, naming the record kind it
/// holds: a home admits registrations alone.
#[test]
fn test_a_registration_whose_record_is_no_registrations_is_refused_naming_its_kind() -> TestResult {
    use ultragraphdb_error_protocol::ProtocolError;
    use ultragraphdb_protocol_wire::{RecordOperation, WriteSetRecord};
    let cases = [
        (
            WriteSetRecord::transaction(0).encode(),
            RecordKind::Transaction,
        ),
        (
            WriteSetRecord::build(0, RecordOperation::CatalogTakesEffect(2)).encode(),
            RecordKind::Build,
        ),
    ];
    for (record, kind) in cases {
        let mut step = step(0, 9)?;
        let out = run(
            &mut step,
            Input::Registration {
                ticket: 5,
                record,
                dictionary: Vec::new(),
            },
            0,
        );
        let synced = run(&mut step, Input::Synced, 0);
        let mut replies = outcomes(&out);
        replies.extend(outcomes(&synced));
        let cause = ProtocolError::field_value(17, "record kind", u64::from(kind.code()));
        assert_eq!(
            replies,
            vec![(5, Err(CommitError::malformed(cause)))],
            "{kind:?}"
        );
        assert_eq!(step.previous_version(), 0, "{kind:?}: no version is taken");
    }
    Ok(())
}

/// A registration finding its epoch full waits for the close, as a commit does, making no session busy, and takes the
/// next epoch's first version.
#[test]
fn test_a_registration_in_a_full_epoch_waits_for_the_close() -> TestResult {
    let fresh = step(0, 9)?;
    let mut step = step(version(10, LAST), 9)?;
    let record = registration(7, 3, 1)?;
    let waited = run(
        &mut step,
        Input::Registration {
            ticket: 5,
            record,
            dictionary: Vec::new(),
        },
        0,
    );
    assert_eq!(waited.append(), None);
    assert!(waited.replies().is_empty());
    assert_eq!(step.waiting(), 1);
    assert_eq!(step.retry(1, 1), fresh.retry(1, 1), "no session is busy");
    let closed = deliver(&mut step, 10)?;
    assert_eq!(
        closed
            .append()
            .map(|(r, _)| (r.first_version(), r.transactions())),
        Some((version(11, 0), 1))
    );
    let synced = run(&mut step, Input::Synced, 0);
    assert_eq!(outcomes(&synced), vec![(5, Ok(version(11, 0)))]);
    Ok(())
}

/// A failed sync stops the step: the round in flight and a registration waiting for the close are both answered
/// unavailable under their tickets, and a close delivered afterwards opens nothing.
#[test]
fn test_a_failed_sync_answers_a_waiting_registration_unavailable_and_ignores_later_closes()
-> TestResult {
    let mut step = step(version(10, LAST - 2), 9)?;
    let first = commit(&mut step, 1, writer(1)?, 0);
    assert_eq!(
        first.append().map(|(r, _)| r.first_version()),
        Some(version(10, LAST))
    );
    let record = registration(7, 3, 1)?;
    let waited = run(
        &mut step,
        Input::Registration {
            ticket: 5,
            record,
            dictionary: Vec::new(),
        },
        0,
    );
    assert!(waited.replies().is_empty());
    let failed = run(&mut step, Input::SyncFailed, 0);
    let mut answered = outcomes(&failed);
    answered.sort_by_key(|(ticket, _)| *ticket);
    assert_eq!(
        answered,
        vec![
            (1, Err(CommitError::unavailable())),
            (5, Err(CommitError::unavailable()))
        ]
    );
    let delivered = step.delivered();
    let late = deliver(&mut step, 10)?;
    assert!(late.replies().is_empty() && late.append().is_none());
    assert_eq!(step.delivered(), delivered, "a stopped step takes no close");
    Ok(())
}

/// A commit relying on a home's reply naming close `c` carries the epoch floor `c + 1` and is held until that epoch is
/// open, so it is stamped there or later (partitions, "A registration's epoch is at most the epoch of every write
/// referencing it"): the scenario's close 11 and a registrant whose latest delivered close is 9.
#[test]
fn test_a_commit_with_an_epoch_floor_waits_for_the_close_before_it() -> TestResult {
    let mut step = step(0, 9)?;
    let mut out = commit(
        &mut step,
        1,
        writer(1)?.with_epoch_floor(Epoch::new(12)?),
        0,
    );
    assert_eq!(out.append(), None);
    assert_eq!(step.held(), 1);
    out = deliver(&mut step, 10)?;
    assert_eq!(out.append(), None, "epoch 11 is open: still held");
    assert_eq!(step.held(), 1);
    out = deliver(&mut step, 11)?;
    assert_eq!(
        out.append().map(|(r, _)| r.first_version()),
        Some(version(12, 0))
    );
    assert_eq!(step.held(), 0);
    out = run(&mut step, Input::Synced, 0);
    assert_eq!(outcomes(&out), vec![(1, Ok(version(12, 0)))]);
    Ok(())
}

/// A commit held for its epoch floor waits on the sequencer's closes, so with none delivered for `close_request_ms` it is
/// refused busy, retryable, as a full epoch's commits are, and a close delivered meanwhile restarts the wait of what stays
/// held: no commit waits on a close without a bound.
#[test]
fn test_a_commit_held_for_its_floor_and_given_no_close_for_close_request_ms_is_refused_busy()
-> TestResult {
    let setting = CloseRequest::new(500, 250).context("the setting")?;
    let mut step = step(0, 9)?.with_close_request(setting);
    let held_at = 1_000;
    let floor = writer(1)?.with_epoch_floor(Epoch::new(12)?);
    let _held = commit(&mut step, 1, floor, held_at);
    assert_eq!(step.held(), 1);
    let mut out = run(
        &mut step,
        Input::Close(close(10, Vec::new(), Vec::new())?),
        held_at + 400_000,
    );
    assert_eq!(out.append(), None, "epoch 11 is open: still held");
    out = run(&mut step, Input::Tick, held_at + 899_999);
    assert!(out.replies().is_empty(), "the close restarted the wait");
    assert_eq!(step.held(), 1);
    out = run(&mut step, Input::Tick, held_at + 900_000);
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
    // The sequencer is not unreachable: a commit arriving next goes on in the open epoch.
    out = commit(&mut step, 2, writer(2)?, held_at + 900_001);
    assert_eq!(
        out.append().map(|(r, _)| r.first_version()),
        Some(version(11, 0))
    );
    Ok(())
}

#[test]
fn test_a_floor_at_or_below_the_open_epoch_holds_nothing() -> TestResult {
    let mut step = step(0, 9)?;
    for (ticket, floor) in [(1, 10), (2, 3)] {
        let out = commit(
            &mut step,
            ticket,
            writer(ticket)?.with_epoch_floor(Epoch::new(floor)?),
            0,
        );
        assert!(out.append().is_some(), "floor {floor} with epoch 10 open");
        let _synced = run(&mut step, Input::Synced, 0);
    }
    assert_eq!(step.held(), 0);
    Ok(())
}

/// The larger of the token's epoch and the epoch floor holds the commit, whichever of the two it is.
#[test]
fn test_the_larger_of_the_token_and_the_floor_holds_the_commit() -> TestResult {
    for (token_epoch, floor) in [(13, 11), (11, 13)] {
        let mut step = step(0, 9)?;
        let carried = writer(1)?
            .with_token(token(1, 40, token_epoch)?)
            .with_epoch_floor(Epoch::new(floor)?);
        let mut out = commit(&mut step, 1, carried, 0);
        assert_eq!(out.append(), None);
        for close in 10..=11 {
            out = deliver(&mut step, close)?;
            assert_eq!(
                out.append(),
                None,
                "token {token_epoch}, floor {floor}: epoch {} is open",
                close + 1
            );
        }
        out = deliver(&mut step, 12)?;
        assert_eq!(
            out.append().map(|(r, _)| r.first_version()),
            Some(version(13, 0)),
            "token {token_epoch}, floor {floor}"
        );
    }
    Ok(())
}

/// The commits held for their floor or their token are refused busy, as the commits waiting for a full epoch's close
/// are, once the close the step asked for goes unanswered for `close_request_ms`: the closes they wait for come from the
/// sequencer counted unreachable.
#[test]
fn test_commits_held_for_a_floor_or_a_token_are_refused_busy_once_the_close_request_goes_unanswered()
-> TestResult {
    let mut step = CommitStep::new(unbounded(), version(10, LAST))
        .with_delivered(Epoch::new(9)?)
        .with_close_request(CloseRequest::new(500, 250).context("the setting")?);
    let asked = 1_000;
    let mut out = commit(&mut step, 1, writer(1)?, asked);
    assert_eq!(out.close_request(), Some(Epoch::new(10)?));
    out = commit(
        &mut step,
        2,
        writer(2)?.with_epoch_floor(Epoch::new(12)?),
        asked,
    );
    assert!(out.replies().is_empty());
    out = commit(&mut step, 3, writer(3)?.with_token(token(1, 4, 12)?), asked);
    assert!(out.replies().is_empty());
    assert_eq!(step.held(), 3);
    out = run(&mut step, Input::Tick, asked + 499_999);
    assert!(out.replies().is_empty());
    out = run(&mut step, Input::Tick, asked + 500_000);
    let mut refused: Vec<(u64, bool)> = out
        .replies()
        .iter()
        .map(|r| {
            (
                r.ticket(),
                r.outcome()
                    .as_ref()
                    .err()
                    .is_some_and(|e| e.code() == ultragraphdb_protocol_wire::ErrorCode::Busy),
            )
        })
        .collect();
    refused.sort_unstable();
    assert_eq!(refused, vec![(1, true), (2, true), (3, true)]);
    assert_eq!(out.released().len(), 3, "each released, writing nothing");
    assert_eq!(step.held(), 0);
    Ok(())
}

/// The dictionary entries a home created for a registration are in its round's dictionary section, as a transaction's
/// are (write-ahead-log, "A round's record is logical": the new dictionary entries the round creates).
#[test]
fn test_a_registrations_dictionary_entries_are_in_its_rounds_dictionary_section() -> TestResult {
    let mut step = step(0, 9)?;
    let entries = vec![
        (b"\x05a-key".to_vec(), b"a-value".to_vec()),
        (b"\x05b-key".to_vec(), b"b-value".to_vec()),
    ];
    let out = run(
        &mut step,
        Input::Registration {
            ticket: 5,
            record: registration(7, 3, 1)?,
            dictionary: entries.clone(),
        },
        0,
    );
    let (round, body) = out.append().context("a round closes")?;
    let header = RoundHeader::new(1, 0, 0, round.first_version(), 1, std::num::NonZeroU32::MIN);
    let view = RoundView::from_body(header, body).context("the round reads")?;
    let logged: Vec<(Vec<u8>, Vec<u8>)> = view
        .dictionary()
        .iter()
        .filter_map(|e| Some((e.key().to_vec(), e.value()?.to_vec())))
        .collect();
    assert_eq!(logged, entries);
    Ok(())
}

/// The step names the registrations its applied round holds under their tickets at their versions, for the caller to
/// apply what their homes left, and the tickets of those it will not apply, refused or dropped with a failed round.
#[test]
fn test_the_step_names_the_registrations_it_applies_and_those_it_dismisses() -> TestResult {
    let registering = |ticket: u64, record: Vec<u8>| Input::Registration {
        ticket,
        record,
        dictionary: Vec::new(),
    };
    let mut step = step(0, 9)?;
    let out = run(&mut step, registering(5, registration(7, 3, 1)?), 0);
    assert!(out.registered().is_empty(), "applied only once synced");
    let malformed = run(&mut step, registering(6, vec![1, 2, 3]), 0);
    assert!(
        malformed.dismissed().is_empty(),
        "it waits for the next round"
    );
    let synced = run(&mut step, Input::Synced, 0);
    assert_eq!(synced.registered(), &[(5, version(10, 0))]);
    assert_eq!(synced.dismissed(), &[6], "refused as its round is decided");
    // A registration in the round in flight when its sync fails, and one waiting, are both dismissed.
    let _in_flight = run(&mut step, registering(7, registration(8, 3, 1)?), 0);
    let _waiting = run(&mut step, registering(8, registration(9, 3, 1)?), 0);
    let failed = run(&mut step, Input::SyncFailed, 0);
    let mut dismissed = failed.dismissed().to_vec();
    dismissed.sort_unstable();
    assert_eq!(dismissed, vec![7, 8]);
    assert!(failed.registered().is_empty());
    // A registration arriving at a stopped step is dismissed at once.
    let refused = run(&mut step, registering(9, registration(10, 3, 1)?), 0);
    assert_eq!(refused.dismissed(), &[9]);
    Ok(())
}
