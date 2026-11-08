/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_storage::{TURN_BYTES, TurnBudget};
use ultragraphdb_tool_bdd::{TestError, TestResult};

/// What a frame costs a turn at least, as the stage's notes state it (`notes/group-9.md`): 16 KiB.
const FRAME_AT_LEAST: usize = 16 * 1024;

/// What a page of keys holding nothing costs a turn, as the stage's notes state it (`notes/group-9.md`): 64 KiB.
const IDLE_PAGE: usize = 64 * 1024;

/// How many times `spend` runs before `budget` is spent, refused past `bound` so a budget that never spends ends the test.
fn spends_until_spent(
    budget: &mut TurnBudget,
    bound: usize,
    spend: impl Fn(&mut TurnBudget),
) -> TestResult<usize> {
    for spent in 0..bound {
        if budget.is_spent() {
            return Ok(spent);
        }
        spend(budget);
    }
    Err(TestError::new(format!(
        "the budget was not spent after {bound} spends"
    )))
}

/// A turn fills its frames to what is left, and ends once its bytes are spent.
#[test]
fn test_a_turn_ends_once_its_bytes_are_spent() {
    let mut budget = TurnBudget::default();
    assert_eq!(budget.fill_to(), TURN_BYTES);
    budget.spend_frame(TURN_BYTES / 2);
    assert_eq!(budget.fill_to(), TURN_BYTES / 2);
    assert!(!budget.is_spent());
    budget.spend_frame(TURN_BYTES / 2);
    assert!(budget.is_spent());
}

/// A frame costs a turn more than its bytes when it is small, so many small Scans end a turn too.
#[test]
fn test_small_frames_end_a_turn_too() -> TestResult {
    let mut budget = TurnBudget::default();
    let frames = spends_until_spent(&mut budget, TURN_BYTES, |budget| budget.spend_frame(1))?;
    assert!(frames > 1, "a turn serves several Scans");
    assert!(frames < TURN_BYTES / 1_024, "{frames} frames of one byte");
    Ok(())
}

/// A frame of one byte costs the turn what a frame costs at least, so a turn serves as many of them as that cost divides it.
#[test]
fn test_a_turn_serves_as_many_small_frames_as_their_cost_divides_it() -> TestResult {
    let mut budget = TurnBudget::default();
    let frames = spends_until_spent(&mut budget, TURN_BYTES, |budget| budget.spend_frame(1))?;
    assert_eq!(frames, TURN_BYTES / FRAME_AT_LEAST);
    Ok(())
}

/// Pages holding nothing spend a turn, so a turn of emptied ranges ends.
#[test]
fn test_idle_pages_end_a_turn() -> TestResult {
    let mut budget = TurnBudget::default();
    let pages = spends_until_spent(&mut budget, TURN_BYTES, TurnBudget::spend_idle_page)?;
    assert!(pages > 1, "a page does not spend a turn whole");
    Ok(())
}

/// A page holding nothing costs the turn its fixed cost, so a turn reads as many such pages as that cost divides it.
#[test]
fn test_a_turn_reads_as_many_idle_pages_as_their_cost_divides_it() -> TestResult {
    let mut budget = TurnBudget::default();
    let pages = spends_until_spent(&mut budget, TURN_BYTES, TurnBudget::spend_idle_page)?;
    assert_eq!(pages, TURN_BYTES / IDLE_PAGE);
    Ok(())
}

/// A turn's last frame is left room enough to be worth its seek, however little is left.
#[test]
fn test_a_turns_last_frame_is_worth_its_seek() {
    let mut budget = TurnBudget::default();
    budget.spend_frame(TURN_BYTES - 1);
    assert!(!budget.is_spent());
    assert!(budget.fill_to() > 1);
}

/// A turn's last frame is filled to what a frame costs at least, not to the little the turn has left.
#[test]
fn test_a_turns_last_frame_fills_to_what_a_frame_costs() {
    let mut budget = TurnBudget::default();
    budget.spend_frame(TURN_BYTES - 1);
    assert_eq!(budget.fill_to(), FRAME_AT_LEAST);
}
