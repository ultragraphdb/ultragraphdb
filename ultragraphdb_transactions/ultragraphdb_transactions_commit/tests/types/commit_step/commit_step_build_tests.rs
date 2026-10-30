/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Builds through the step (decided 2026-10-28): a proposal starting a build waits, set aside, while a build runs,
//! and is decided again once the build's end is admitted; a build's end is a round of its own.

use ultragraphdb_transactions_commit::{
    CommitRequest, CommitStep, Input, Proposal, Reply, RoundBounds, StepOutput,
};
use ultragraphdb_transactions_txn::ReadSet;

use crate::utils::proposal_double::{Reads, TestProposal, Writes, proposal};

/// A proposal that starts a build, ends one, or neither.
#[derive(Debug, Clone)]
struct BuildProposal {
    inner: TestProposal,
    starts: bool,
    ends: bool,
}

impl Proposal for BuildProposal {
    fn read_set(&self) -> &ReadSet {
        &self.inner.read_set
    }

    fn record(&self) -> &[u8] {
        &self.inner.record
    }

    fn starts_build(&self) -> bool {
        self.starts
    }

    fn ends_build(&self) -> bool {
        self.ends
    }
}

type Step = CommitStep<BuildProposal>;
type Out = StepOutput<BuildProposal>;

fn of(key: &[u8], starts: bool, ends: bool) -> BuildProposal {
    BuildProposal {
        inner: proposal(0, &Reads::default(), &Writes::point(key)),
        starts,
        ends,
    }
}

fn commit(step: &mut Step, out: &mut Out, ticket: u64, p: BuildProposal) {
    step.step(Input::Commit(CommitRequest::new(ticket, p)), 0, out);
}

fn sync(step: &mut Step, out: &mut Out) {
    step.step(Input::Synced, 0, out);
}

fn unbounded() -> RoundBounds {
    RoundBounds::new(u32::MAX, u32::MAX)
}

/// The transactions of the round `out` asks to append.
fn round_size(out: &Out) -> Option<u32> {
    out.append().map(|(record, _)| record.transactions())
}

#[test]
fn test_a_proposal_starting_a_build_waits_while_one_runs_and_is_decided_after_its_end() {
    let mut step = Step::new(unbounded(), 0);
    let mut out = Out::new();
    commit(&mut step, &mut out, 1, of(b"a", true, false));
    assert!(step.is_building(), "admitted, the build runs");
    sync(&mut step, &mut out);
    // A second build waits; the plain write beside it is decided.
    commit(&mut step, &mut out, 2, of(b"b", true, false));
    assert_eq!(round_size(&out), Some(0), "set aside, nothing to decide");
    assert_eq!((step.waiting(), step.deferred()), (0, 1));
    sync(&mut step, &mut out);
    commit(&mut step, &mut out, 3, of(b"c", false, false));
    assert_eq!(round_size(&out), Some(1));
    sync(&mut step, &mut out);
    assert_eq!(
        out.replies(),
        &[Reply::new(3, Ok(4))],
        "a round deciding nothing takes no version"
    );
    // The build's end: its round alone; the proposal set aside is decided in the next.
    commit(&mut step, &mut out, 4, of(b"end", false, true));
    assert_eq!(round_size(&out), Some(1));
    assert!(!step.is_building());
    assert_eq!(
        (step.waiting(), step.deferred()),
        (1, 0),
        "back in the queue"
    );
    sync(&mut step, &mut out);
    assert_eq!(out.replies(), &[Reply::new(4, Ok(6))]);
    assert_eq!(round_size(&out), Some(1), "the next round decides it");
    assert!(step.is_building());
    sync(&mut step, &mut out);
    assert_eq!(out.replies(), &[Reply::new(2, Ok(8))]);
}

#[test]
fn test_a_builds_end_is_a_round_of_its_own() {
    let mut step = Step::new(unbounded(), 0).with_build_running(true);
    let mut out = Out::new();
    // A round in flight, so the next three wait together.
    commit(&mut step, &mut out, 1, of(b"x", false, false));
    commit(&mut step, &mut out, 2, of(b"a", false, false));
    commit(&mut step, &mut out, 3, of(b"end", false, true));
    commit(&mut step, &mut out, 4, of(b"b", false, false));
    sync(&mut step, &mut out);
    assert_eq!(round_size(&out), Some(1), "the round closes before the end");
    sync(&mut step, &mut out);
    assert_eq!(out.replies(), &[Reply::new(2, Ok(4))]);
    assert_eq!(round_size(&out), Some(1), "the end alone");
    sync(&mut step, &mut out);
    assert_eq!(out.replies(), &[Reply::new(3, Ok(6))]);
    assert_eq!(round_size(&out), Some(1));
    sync(&mut step, &mut out);
    assert_eq!(out.replies(), &[Reply::new(4, Ok(8))]);
}

#[test]
fn test_a_failed_sync_answers_the_proposals_set_aside_unavailable() {
    let mut step = Step::new(unbounded(), 0).with_build_running(true);
    let mut out = Out::new();
    commit(&mut step, &mut out, 1, of(b"x", false, false));
    commit(&mut step, &mut out, 2, of(b"b", true, false));
    sync(&mut step, &mut out);
    assert_eq!(step.deferred(), 1);
    step.step(Input::SyncFailed, 0, &mut out);
    assert_eq!(step.deferred(), 0);
    assert!(
        out.replies()
            .iter()
            .any(|r| r.ticket() == 2 && r.outcome().is_err())
    );
    assert!(out.released().iter().any(|(t, _)| *t == 2));
}
