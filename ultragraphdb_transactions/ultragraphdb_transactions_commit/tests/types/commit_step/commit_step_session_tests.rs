/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Sessions through the commit step (D9, 7.8; transactions, "Exactly-once commits through sessions"): a session recorded
//! at its first commit in the partition, checked first, in the Lean model's order (a retry of the last committed request
//! answered with its stored reply, a number below the last refused, numbers above it with gaps taken), one commit in
//! flight per session. Every version is written from the layout `epoch << 22 | sequence`: a step after version 0
//! stamps epoch 1 from sequence 0.

use ultragraphdb_protocol_wire::{ConflictKind, ErrorCode};
use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_transactions_commit::{
    CommitError, CommitRequest, CommitStep, Input, Reply, RoundBounds, StepOutput,
};

use crate::utils::fixture::version;
use crate::utils::proposal_double::{Reads, TestProposal, Writes, proposal_in};

type Step = CommitStep<TestProposal>;
type Out = StepOutput<TestProposal>;

/// The session the tests commit through: its identity uses the high 64 bits.
const S: u128 = 0x0102 << 64;

fn unbounded() -> RoundBounds {
    RoundBounds::new(u32::MAX, u32::MAX)
}

fn commit(step: &mut Step, out: &mut Out, ticket: u64, p: TestProposal) {
    step.step(Input::Commit(CommitRequest::new(ticket, p)), 0, out);
}

fn sync(step: &mut Step, out: &mut Out) {
    step.step(Input::Synced, 0, out);
}

fn blind(session: u128, request: u64, key: &[u8]) -> TestResult<TestProposal> {
    proposal_in(session, request, 0, &Reads::default(), &Writes::point(key))
}

/// The outcomes of `out`'s replies by ticket.
fn outcomes(out: &Out) -> Vec<(u64, Result<u64, CommitError>)> {
    out.replies()
        .iter()
        .map(|r| (r.ticket(), r.outcome().clone()))
        .collect()
}

#[test]
fn test_a_session_is_recorded_at_its_first_commit_in_the_partition() -> TestResult {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    assert_eq!(step.sessions().last(S), None);
    commit(&mut step, &mut out, 1, blind(S, 5, b"k")?);
    assert_eq!(step.sessions().last(S), None, "held once applied");
    sync(&mut step, &mut out);
    assert_eq!(outcomes(&out), vec![(1, Ok(version(1, 0)))]);
    assert_eq!(step.sessions().last(S), Some((5, version(1, 0))));
    Ok(())
}

#[test]
fn test_a_retry_of_the_last_committed_request_is_answered_with_its_reply_and_commits_nothing()
-> TestResult {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    commit(&mut step, &mut out, 1, blind(S, 1, b"k")?);
    sync(&mut step, &mut out);
    let original: Reply = out
        .replies()
        .first()
        .cloned()
        .ok_or_else(|| ultragraphdb_tool_bdd::TestError::new("the original reply"))?;
    // The retry is answered at once, released, and no round is logged for it.
    let entries = step.resolver().history().len();
    commit(&mut step, &mut out, 2, blind(S, 1, b"k")?);
    assert_eq!(outcomes(&out), vec![(2, Ok(version(1, 0)))]);
    assert_eq!(
        out.replies().first().and_then(Reply::token),
        original.token()
    );
    assert_eq!(out.released().len(), 1);
    assert_eq!(out.append(), None);
    assert_eq!(step.in_flight(), None);
    assert_eq!(step.resolver().history().len(), entries, "no writes added");
    assert_eq!(step.previous_version(), version(1, 0), "no version taken");
    Ok(())
}

#[test]
fn test_a_number_below_the_last_is_refused_naming_the_last_and_gaps_are_taken() -> TestResult {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    commit(&mut step, &mut out, 1, blind(S, 4, b"a")?);
    sync(&mut step, &mut out);
    commit(&mut step, &mut out, 2, blind(S, 3, b"b")?);
    sync(&mut step, &mut out);
    let refused = CommitError::request_out_of_order(4);
    assert_eq!(outcomes(&out), vec![(2, Err(refused.clone()))]);
    assert_eq!(refused.code(), ErrorCode::RequestOutOfOrder);
    assert_eq!(refused.detail(), 4);
    assert!(!refused.is_retryable());
    // 9, above the last with a gap the session's commits elsewhere left, is taken.
    commit(&mut step, &mut out, 3, blind(S, 9, b"c")?);
    sync(&mut step, &mut out);
    assert_eq!(outcomes(&out), vec![(3, Ok(version(1, 2)))]);
    assert_eq!(step.sessions().last(S), Some((9, version(1, 2))));
    Ok(())
}

#[test]
fn test_a_session_has_at_most_one_commit_decided_and_not_applied() -> TestResult {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    // A round in flight, then two requests of the session in the next round: the second is busy.
    commit(&mut step, &mut out, 1, blind(0, 0, b"x")?);
    commit(&mut step, &mut out, 2, blind(S, 1, b"a")?);
    commit(&mut step, &mut out, 3, blind(S, 2, b"b")?);
    sync(&mut step, &mut out);
    // Its retry, sent while the first is in flight, is decided in the round after, once the first is applied: it is
    // answered with the first's version.
    commit(&mut step, &mut out, 4, blind(S, 1, b"a")?);
    sync(&mut step, &mut out);
    let busy = CommitError::session_busy(S);
    assert_eq!(
        outcomes(&out),
        vec![
            (2, Ok(version(1, 2))),
            (3, Err(busy.clone())),
            (4, Ok(version(1, 2)))
        ]
    );
    assert_eq!(busy.code(), ErrorCode::SessionBusy);
    assert_eq!(busy.detail(), 0);
    assert!(busy.is_retryable());
    // Once applied, the next number commits.
    commit(&mut step, &mut out, 5, blind(S, 2, b"b")?);
    sync(&mut step, &mut out);
    assert_eq!(outcomes(&out), vec![(5, Ok(version(1, 4)))]);
    Ok(())
}

#[test]
fn test_an_aborted_request_does_not_advance_its_session() -> TestResult {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    commit(&mut step, &mut out, 1, blind(0, 0, b"k")?);
    let reads_k = proposal_in(S, 1, 0, &Reads::list(b"k"), &Writes::point(b"x"))?;
    commit(&mut step, &mut out, 2, reads_k);
    sync(&mut step, &mut out);
    sync(&mut step, &mut out);
    assert_eq!(
        outcomes(&out),
        vec![(2, Err(CommitError::conflict(ConflictKind::List)))]
    );
    assert_eq!(step.sessions().last(S), None);
    let retry = proposal_in(
        S,
        1,
        version(1, 0),
        &Reads::list(b"k"),
        &Writes::point(b"x"),
    )?;
    commit(&mut step, &mut out, 3, retry);
    sync(&mut step, &mut out);
    assert_eq!(outcomes(&out), vec![(3, Ok(version(1, 2)))]);
    Ok(())
}

#[test]
fn test_a_failed_sync_answers_unavailable_and_releases_the_commits_under_their_tickets()
-> TestResult {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    commit(&mut step, &mut out, 1, blind(0, 0, b"x")?);
    commit(&mut step, &mut out, 3, blind(S, 1, b"y")?);
    sync(&mut step, &mut out);
    step.step(Input::SyncFailed, 0, &mut out);
    assert_eq!(outcomes(&out), vec![(3, Err(CommitError::unavailable()))]);
    let released: Vec<u64> = out.released().iter().map(|(t, _)| *t).collect();
    assert_eq!(released, vec![3]);
    assert!(step.sessions().is_empty());
    commit(&mut step, &mut out, 4, blind(S, 2, b"z")?);
    assert_eq!(outcomes(&out), vec![(4, Err(CommitError::unavailable()))]);
    Ok(())
}

#[test]
fn test_a_retry_is_answered_before_its_reads_are_validated() -> TestResult {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    let original = proposal_in(S, 1, 0, &Reads::list(b"k"), &Writes::point(b"x"))?;
    commit(&mut step, &mut out, 1, original);
    sync(&mut step, &mut out);
    commit(&mut step, &mut out, 2, blind(0, 0, b"k")?);
    sync(&mut step, &mut out);
    let retry = proposal_in(S, 1, 0, &Reads::list(b"k"), &Writes::point(b"x"))?;
    commit(&mut step, &mut out, 3, retry);
    assert_eq!(outcomes(&out), vec![(3, Ok(version(1, 0)))]);
    Ok(())
}

/// A step with a session table of one session takes its first session's commits, its next requests included, and
/// refuses a second session's busy, naming the capacity; session 0, no session, is always taken. Its bounds are the
/// ones it was made with.
#[test]
fn test_a_full_session_table_refuses_a_new_session_busy() -> TestResult {
    let bounds = RoundBounds::new(4_096, 3);
    let mut step = Step::new(bounds, 0).with_sessions(1);
    assert_eq!(step.bounds(), bounds);
    let mut out = Out::new();
    let mut answered = Vec::new();
    commit(&mut step, &mut out, 1, blind(S, 1, b"a")?);
    answered.extend(outcomes(&out));
    sync(&mut step, &mut out);
    answered.extend(outcomes(&out));
    for (ticket, session, request, key) in [(2, S + 1, 1, b"b"), (3, S, 2, b"c"), (4, 0, 0, b"d")] {
        commit(&mut step, &mut out, ticket, blind(session, request, key)?);
        answered.extend(outcomes(&out));
    }
    // 3's round, in flight when 4 arrives, then 4's.
    for _ in 0..2 {
        sync(&mut step, &mut out);
        answered.extend(outcomes(&out));
    }
    answered.sort_by_key(|(ticket, _)| *ticket);
    assert_eq!(
        answered,
        vec![
            (1, Ok(version(1, 0))),
            (2, Err(CommitError::busy(1))),
            (3, Ok(version(1, 2))),
            (4, Ok(version(1, 4))),
        ]
    );
    assert_eq!(step.sessions().len(), 1);
    Ok(())
}
