/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_log::Position;
use ultragraphdb_transactions_commit::{CommitRequest, CommitStep, Input, RoundBounds, StepOutput};

use crate::utils::proposal_double::{Reads, TestProposal, Writes, proposal};

#[test]
fn test_a_new_output_asks_for_nothing() {
    let out = StepOutput::<TestProposal>::new();
    assert_eq!(out.append(), None);
    assert_eq!(out.round_ref(Position::new(1), 0), None);
    assert_eq!(out.applied(), None);
    assert!(out.apply().is_empty());
    assert!(out.replies().is_empty());
    let default = StepOutput::<TestProposal>::default();
    assert_eq!(default.append(), None);
}

#[test]
fn test_clearing_keeps_the_capacity() {
    let mut step = CommitStep::new(RoundBounds::new(u32::MAX, u32::MAX), 0);
    let mut out = StepOutput::new();
    let p = proposal(0, &Reads::default(), &Writes::point(b"k"));
    step.step(Input::Commit(CommitRequest::new(1, p)), 0, &mut out);
    step.step(Input::Synced, 0, &mut out);
    let capacity = out.capacity();
    assert!(
        capacity.0 > 0 && capacity.1 > 0 && capacity.2 > 0,
        "{capacity:?}"
    );
    out.clear();
    assert_eq!(out.capacity(), capacity);
    assert_eq!(out.applied(), None);
    assert!(out.apply().is_empty() && out.replies().is_empty());
}
