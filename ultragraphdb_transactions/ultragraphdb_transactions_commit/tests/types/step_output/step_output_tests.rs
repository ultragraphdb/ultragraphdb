/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_log::Position;
use ultragraphdb_transactions_commit::{CommitRequest, CommitStep, Input, RoundBounds, StepOutput};

use crate::utils::proposal_double::{Reads, TestProposal, Writes, proposal};

#[test]
fn test_a_new_output_asks_for_nothing() -> TestResult {
    let out = StepOutput::<TestProposal>::new();
    assert_eq!(out.append(), None);
    assert_eq!(out.round_ref(0, std::num::NonZeroU32::MIN), None);
    assert_eq!(out.position(), None);
    assert_eq!(out.close_request(), None);
    assert_eq!(out.applied(), None);
    assert!(out.apply().is_empty());
    assert!(out.replies().is_empty());
    let default = StepOutput::<TestProposal>::default();
    assert_eq!(default.append(), None);
    Ok(())
}

#[test]
fn test_clearing_keeps_the_capacity() -> TestResult {
    let mut step = CommitStep::new(RoundBounds::new(u32::MAX, u32::MAX), 0);
    let mut out = StepOutput::new();
    let p = proposal(0, &Reads::default(), &Writes::point(b"k"))?;
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
    Ok(())
}

#[test]
fn test_a_closed_round_is_appended_at_the_position_after_the_tail() -> TestResult {
    let mut step = CommitStep::new(RoundBounds::new(u32::MAX, u32::MAX), 0).with_tail(6);
    let mut out = StepOutput::new();
    let p = proposal(0, &Reads::default(), &Writes::point(b"k"))?;
    step.step(Input::Commit(CommitRequest::new(1, p)), 0, &mut out);
    assert_eq!(out.position(), Some(Position::new(7)));
    let round = out
        .round_ref(0xAB, std::num::NonZeroU32::new(3).context("3")?)
        .ok_or_else(|| ultragraphdb_tool_bdd::TestError::new("a round to append"))?;
    assert_eq!(round.position(), Position::new(7));
    assert_eq!(round.parent(), 0xAB);
    assert_eq!(round.derivation().get(), 3);
    assert_eq!(round.transactions(), 1);
    // Version 0 is epoch 0, so the first commit takes epoch 1, sequence 0: 1 << 22.
    assert_eq!(round.first_version(), 4_194_304);
    assert_eq!(step.tail(), 7);
    out.clear();
    assert_eq!(out.position(), None);
    Ok(())
}
