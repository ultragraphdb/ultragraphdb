/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Aborted proposals give their buffers back (approved 2026-10-28): every proposal the step will not apply leaves it
//! through [`StepOutput::released`] under its ticket, in the step that decides so, as a committed one leaves through
//! the apply.

use ultragraphdb_transactions_commit::{CommitRequest, CommitStep, Input, RoundBounds, StepOutput};

use crate::utils::proposal_double::{Reads, TestProposal, Writes, proposal};

type Step = CommitStep<TestProposal>;
type Out = StepOutput<TestProposal>;

fn unbounded() -> RoundBounds {
    RoundBounds::new(u32::MAX, u32::MAX)
}

fn commit(step: &mut Step, out: &mut Out, ticket: u64, p: TestProposal) {
    step.step(Input::Commit(CommitRequest::new(ticket, p)), 0, out);
}

fn released(out: &Out) -> Vec<(u64, Vec<u8>)> {
    out.released()
        .iter()
        .map(|(ticket, p)| (*ticket, p.record.clone()))
        .collect()
}

#[test]
fn test_an_aborted_proposal_is_released_when_its_round_is_decided() {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    commit(
        &mut step,
        &mut out,
        1,
        proposal(0, &Reads::default(), &Writes::point(b"k")),
    );
    assert!(
        out.released().is_empty(),
        "a commit is applied, not released"
    );
    // Both wait for the sync of the first round; the first of them conflicts with it.
    let loser = proposal(0, &Reads::list(b"k"), &Writes::point(b"x"));
    let winner = proposal(0, &Reads::list(b"j"), &Writes::point(b"y"));
    commit(&mut step, &mut out, 2, loser.clone());
    commit(&mut step, &mut out, 3, winner);
    assert!(out.released().is_empty());
    step.step(Input::Synced, 0, &mut out);
    assert_eq!(released(&out), vec![(2, loser.record.clone())]);
    assert_eq!(out.apply().len(), 1, "the first round's commit");
    // The step after clears it.
    step.step(Input::Synced, 0, &mut out);
    assert!(out.released().is_empty());
    assert_eq!(out.apply().len(), 1, "the second round's commit");
}

#[test]
fn test_a_request_refused_on_arrival_is_released() {
    let mut big = proposal(0, &Reads::default(), &Writes::point(b"big"));
    big.record = crate::utils::proposal_double::record(0, 0, &Writes::point(b"big"), 9_000);
    let mut step = Step::new(RoundBounds::new(4_096, u32::MAX), 0);
    let mut out = Out::new();
    commit(&mut step, &mut out, 5, big.clone());
    assert_eq!(released(&out), vec![(5, big.record)]);
}

#[test]
fn test_a_failed_sync_releases_the_round_in_flight_the_waiting_and_every_later_request() {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    let p = |key: &[u8]| proposal(0, &Reads::default(), &Writes::point(key));
    commit(&mut step, &mut out, 1, p(b"a"));
    commit(&mut step, &mut out, 2, p(b"b"));
    commit(&mut step, &mut out, 3, p(b"c"));
    step.step(Input::SyncFailed, 0, &mut out);
    let tickets: Vec<u64> = out.released().iter().map(|(t, _)| *t).collect();
    assert_eq!(tickets, vec![1, 2, 3]);
    assert!(out.apply().is_empty());
    commit(&mut step, &mut out, 4, p(b"d"));
    assert_eq!(released(&out), vec![(4, p(b"d").record)]);
}

#[test]
fn test_released_proposals_are_drained_by_the_caller_and_the_buffer_kept() {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    commit(
        &mut step,
        &mut out,
        1,
        proposal(0, &Reads::default(), &Writes::point(b"k")),
    );
    for round in 0..4u64 {
        let ticket = 10 + round;
        commit(
            &mut step,
            &mut out,
            ticket,
            proposal(0, &Reads::list(b"k"), &Writes::point(b"x")),
        );
        step.step(Input::Synced, 0, &mut out);
        let drained: Vec<u64> = out.released_mut().drain(..).map(|(t, _)| t).collect();
        assert_eq!(drained, vec![ticket]);
        assert!(
            out.released_mut().capacity() >= 1,
            "the drained buffer is kept"
        );
        // A further write to `k` makes the next one conflict again.
        commit(
            &mut step,
            &mut out,
            100 + round,
            proposal(0, &Reads::default(), &Writes::point(b"k")),
        );
        step.step(Input::Synced, 0, &mut out);
    }
}

/// The round in flight's admitted proposals are released under their own tickets when its sync fails, an abort before
/// them in the round having been released when the round closed.
#[test]
fn test_a_failed_sync_releases_each_admitted_proposal_under_its_own_ticket() {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    commit(
        &mut step,
        &mut out,
        1,
        proposal(0, &Reads::default(), &Writes::point(b"k")),
    );
    let loser = proposal(0, &Reads::list(b"k"), &Writes::point(b"x"));
    let winner = proposal(0, &Reads::default(), &Writes::point(b"y"));
    commit(&mut step, &mut out, 2, loser.clone());
    commit(&mut step, &mut out, 3, winner.clone());
    step.step(Input::Synced, 0, &mut out);
    assert_eq!(released(&out), vec![(2, loser.record)]);
    step.step(Input::SyncFailed, 0, &mut out);
    assert_eq!(released(&out), vec![(3, winner.record)]);
}

/// A commit arriving to a full commit queue is refused busy, retryable, and released at once.
#[test]
fn test_a_commit_to_a_full_queue_is_refused_busy_and_released() {
    let mut step = Step::new(unbounded(), 0).with_queue(1);
    let mut out = Out::new();
    let p = |key: &[u8]| proposal(0, &Reads::default(), &Writes::point(key));
    commit(&mut step, &mut out, 1, p(b"a"));
    commit(&mut step, &mut out, 2, p(b"b"));
    assert!(out.replies().is_empty(), "the queue holds one");
    commit(&mut step, &mut out, 3, p(b"c"));
    let busy = ultragraphdb_transactions_commit::CommitError::Busy(1);
    assert_eq!(
        out.replies(),
        &[ultragraphdb_transactions_commit::Reply::new(
            3,
            Err(busy.clone())
        )]
    );
    assert_eq!(released(&out), vec![(3, p(b"c").record)]);
    assert!(busy.is_retryable());
    assert_eq!(busy.code(), ultragraphdb_protocol_wire::ErrorCode::Busy);
    assert_eq!(busy.detail(), 0);
    assert!(busy.to_string().contains("full"));
    assert_eq!(step.waiting(), 1);
}
