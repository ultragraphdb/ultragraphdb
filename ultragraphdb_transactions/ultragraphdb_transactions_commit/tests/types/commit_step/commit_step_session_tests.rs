/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Sessions through the commit step (D9, 7.8): registered through a round and numbered by its version, checked first,
//! in the Lean model's order (a retry of the last committed request answered with its version, a number out of order
//! refused), one commit in flight per session, the table bounded with the oldest evicted.

use ultragraphdb_protocol_wire::{BodyBuilder, ConflictKind, ErrorCode, RoundBody, WriteSetRecord};
use ultragraphdb_transactions_commit::{
    CommitError, CommitRequest, CommitStep, Input, Reply, RoundBounds, StepOutput,
};

use crate::utils::proposal_double::{Reads, TestProposal, Writes, proposal_in};

type Step = CommitStep<TestProposal>;
type Out = StepOutput<TestProposal>;

fn unbounded() -> RoundBounds {
    RoundBounds::new(u32::MAX, u32::MAX)
}

fn commit(step: &mut Step, out: &mut Out, ticket: u64, p: TestProposal) {
    step.step(Input::Commit(CommitRequest::new(ticket, p)), 0, out);
}

fn sync(step: &mut Step, out: &mut Out) {
    step.step(Input::Synced, 0, out);
}

/// A step after version 100 holding session 102, registered in a round of its own.
fn registered() -> (Step, Out, u64) {
    let mut step = Step::new(unbounded(), 100);
    let mut out = Out::new();
    step.step(Input::Register(9), 0, &mut out);
    assert!(out.replies().is_empty(), "answered once applied");
    assert_eq!(step.sessions().last(102), None, "held once applied");
    sync(&mut step, &mut out);
    assert_eq!(out.replies(), &[Reply::new(9, Ok(102))]);
    assert_eq!(step.sessions().last(102), Some((0, 0)));
    (step, out, 102)
}

fn blind(session: u64, request: u64, key: &[u8]) -> TestProposal {
    proposal_in(
        session,
        request,
        100,
        &Reads::default(),
        &Writes::point(key),
    )
}

#[test]
fn test_a_registration_takes_a_version_which_numbers_the_session_and_its_record_is_logged() {
    let mut step = Step::new(unbounded(), 100);
    let mut out = Out::new();
    commit(&mut step, &mut out, 1, blind(0, 0, b"a"));
    step.step(Input::Register(2), 0, &mut out);
    commit(&mut step, &mut out, 3, blind(0, 0, b"b"));
    sync(&mut step, &mut out);
    let (record, body) = out.append().expect("the second round");
    assert_eq!(record.transactions(), 2);
    assert_eq!(record.committed(), 2);
    assert_eq!(record.first_version(), 104);
    // The wire's own builder: the registration's record (session 104, request 0, no writes), then the commit's.
    let mut expected = RoundBody::new(0, 0, 0, 104);
    expected.commit(WriteSetRecord::new(0, 0, 104, 0).encode());
    expected.commit(blind(0, 0, b"b").record);
    assert_eq!(body, expected.body().as_slice());
    sync(&mut step, &mut out);
    assert_eq!(
        out.replies(),
        &[Reply::new(2, Ok(104)), Reply::new(3, Ok(106))]
    );
    assert_eq!(step.sessions().len(), 1);
    assert_eq!(out.apply().len(), 1, "a registration applies nothing");
}

#[test]
fn test_a_retry_of_the_last_committed_request_is_answered_with_its_version_and_commits_nothing() {
    let (mut step, mut out, s) = registered();
    commit(&mut step, &mut out, 1, blind(s, 1, b"k"));
    sync(&mut step, &mut out);
    assert_eq!(out.replies(), &[Reply::new(1, Ok(104))]);
    assert_eq!(step.sessions().last(s), Some((1, 104)));
    // The retry closes a round at once: answered there, released, and the round holds no decision for it.
    let entries = step.resolver().history().len();
    commit(&mut step, &mut out, 2, blind(s, 1, b"k"));
    assert_eq!(out.replies(), &[Reply::new(2, Ok(104))]);
    assert_eq!(out.released().len(), 1);
    assert_eq!(out.append().map(|(r, _)| r.transactions()), Some(0));
    assert_eq!(step.resolver().history().len(), entries, "no writes added");
    sync(&mut step, &mut out);
    assert!(out.apply().is_empty());
    assert_eq!(step.previous_version(), 104, "no version taken");
}

#[test]
fn test_a_request_number_out_of_order_is_refused_naming_the_one_expected() {
    let (mut step, mut out, s) = registered();
    commit(&mut step, &mut out, 1, blind(s, 0, b"a"));
    commit(&mut step, &mut out, 2, blind(s, 2, b"b"));
    sync(&mut step, &mut out);
    sync(&mut step, &mut out);
    let refused = CommitError::RequestOutOfOrder(1);
    assert_eq!(
        out.replies(),
        &[Reply::new(2, Err(refused.clone()))],
        "the second round's"
    );
    assert_eq!(refused.code(), ErrorCode::RequestOutOfOrder);
    assert_eq!(refused.detail(), 1);
    assert!(!refused.is_retryable());
    let (mut step, mut out, s) = registered();
    commit(&mut step, &mut out, 1, blind(s, 1, b"a"));
    sync(&mut step, &mut out);
    commit(&mut step, &mut out, 2, blind(s, 3, b"b"));
    sync(&mut step, &mut out);
    assert_eq!(
        out.replies(),
        &[Reply::new(2, Err(CommitError::RequestOutOfOrder(2)))]
    );
}

#[test]
fn test_a_session_has_at_most_one_commit_in_flight() {
    let (mut step, mut out, s) = registered();
    // A round in flight, then two requests of the session in the next round: the second is busy.
    commit(&mut step, &mut out, 1, blind(0, 0, b"x"));
    commit(&mut step, &mut out, 2, blind(s, 1, b"a"));
    commit(&mut step, &mut out, 3, blind(s, 2, b"b"));
    sync(&mut step, &mut out);
    // Its retry, sent while the first is in flight, is decided in the round after, once the first is applied: it is
    // answered with the first's version.
    commit(&mut step, &mut out, 4, blind(s, 1, b"a"));
    sync(&mut step, &mut out);
    let busy = CommitError::SessionBusy(s);
    assert_eq!(
        out.replies(),
        &[
            Reply::new(2, Ok(106)),
            Reply::new(3, Err(busy.clone())),
            Reply::new(4, Ok(106))
        ]
    );
    assert_eq!(busy.code(), ErrorCode::SessionBusy);
    assert_eq!(busy.detail(), s);
    sync(&mut step, &mut out);
    // Once applied, the next number commits.
    commit(&mut step, &mut out, 5, blind(s, 2, b"b"));
    sync(&mut step, &mut out);
    assert_eq!(out.replies(), &[Reply::new(5, Ok(108))]);
}

#[test]
fn test_an_aborted_request_does_not_advance_its_session() {
    let (mut step, mut out, s) = registered();
    commit(&mut step, &mut out, 1, blind(0, 0, b"k"));
    let reads_k = proposal_in(s, 1, 100, &Reads::list(b"k"), &Writes::point(b"x"));
    commit(&mut step, &mut out, 2, reads_k);
    sync(&mut step, &mut out);
    sync(&mut step, &mut out);
    assert_eq!(
        out.replies(),
        &[Reply::new(
            2,
            Err(CommitError::Conflict(ConflictKind::List))
        )]
    );
    assert_eq!(step.sessions().last(s), Some((0, 0)));
    let retry = proposal_in(s, 1, 104, &Reads::list(b"k"), &Writes::point(b"x"));
    commit(&mut step, &mut out, 3, retry);
    sync(&mut step, &mut out);
    assert_eq!(out.replies(), &[Reply::new(3, Ok(106))]);
}

#[test]
fn test_a_full_table_evicts_the_oldest_session_whose_requests_are_then_refused() {
    let mut step = Step::new(unbounded(), 100).with_sessions(2);
    let mut out = Out::new();
    for ticket in 1..=2 {
        step.step(Input::Register(ticket), 0, &mut out);
        sync(&mut step, &mut out);
    }
    let (first, second) = (102, 104);
    // The first commits after the second registered, so the second is the oldest.
    commit(&mut step, &mut out, 3, blind(first, 1, b"a"));
    sync(&mut step, &mut out);
    step.step(Input::Register(4), 0, &mut out);
    sync(&mut step, &mut out);
    assert_eq!(out.replies(), &[Reply::new(4, Ok(108))]);
    assert_eq!(step.sessions().len(), 2);
    assert_eq!(step.sessions().last(second), None, "evicted");
    commit(&mut step, &mut out, 5, blind(second, 1, b"b"));
    sync(&mut step, &mut out);
    let evicted = CommitError::SessionEvicted(second);
    assert_eq!(out.replies(), &[Reply::new(5, Err(evicted.clone()))]);
    assert_eq!(evicted.code(), ErrorCode::SessionEvicted);
    assert_eq!(evicted.detail(), second);
    assert!(!evicted.is_retryable());
    assert!(evicted.to_string().contains("evicted"));
    // An unregistered session reads as evicted, and session 0 is no session.
    commit(&mut step, &mut out, 6, blind(999, 1, b"c"));
    commit(&mut step, &mut out, 7, blind(0, 0, b"d"));
    sync(&mut step, &mut out);
    let mut replies = out.replies().to_vec();
    sync(&mut step, &mut out);
    replies.extend_from_slice(out.replies());
    assert_eq!(
        replies,
        vec![
            Reply::new(6, Err(CommitError::SessionEvicted(999))),
            Reply::new(7, Ok(110))
        ]
    );
}

#[test]
fn test_a_failed_sync_answers_a_registration_unavailable_and_releases_the_commits_under_their_tickets()
 {
    let mut step = Step::new(unbounded(), 100);
    let mut out = Out::new();
    commit(&mut step, &mut out, 1, blind(0, 0, b"x"));
    step.step(Input::Register(2), 0, &mut out);
    commit(&mut step, &mut out, 3, blind(0, 0, b"y"));
    sync(&mut step, &mut out);
    step.step(Input::SyncFailed, 0, &mut out);
    assert_eq!(
        out.replies(),
        &[
            Reply::new(2, Err(CommitError::Unavailable())),
            Reply::new(3, Err(CommitError::Unavailable()))
        ]
    );
    let released: Vec<u64> = out.released().iter().map(|(t, _)| *t).collect();
    assert_eq!(released, vec![3]);
    assert!(step.sessions().is_empty());
    step.step(Input::Register(4), 0, &mut out);
    assert_eq!(
        out.replies(),
        &[Reply::new(4, Err(CommitError::Unavailable()))]
    );
}

#[test]
fn test_the_session_errors_read_as_their_reasons() {
    assert!(
        CommitError::SessionBusy(5)
            .to_string()
            .contains("in flight")
    );
    assert!(
        CommitError::RequestOutOfOrder(8)
            .to_string()
            .contains("8 is expected")
    );
    assert!(std::error::Error::source(&CommitError::SessionBusy(5)).is_none());
}

/// The session is checked before validation (the Lean model's order): a retry is answered with the stored reply even
/// when its read set conflicts with a commit after the original, which it would otherwise abort on.
#[test]
fn test_a_retry_is_answered_before_its_reads_are_validated() {
    let (mut step, mut out, s) = registered();
    let original = proposal_in(s, 1, 100, &Reads::list(b"k"), &Writes::point(b"x"));
    commit(&mut step, &mut out, 1, original);
    sync(&mut step, &mut out);
    commit(&mut step, &mut out, 2, blind(0, 0, b"k"));
    sync(&mut step, &mut out);
    // The retry read `k` at 100, and `k` was written at 106 since.
    let retry = proposal_in(s, 1, 100, &Reads::list(b"k"), &Writes::point(b"x"));
    commit(&mut step, &mut out, 3, retry);
    assert_eq!(out.replies(), &[Reply::new(3, Ok(104))]);
}
