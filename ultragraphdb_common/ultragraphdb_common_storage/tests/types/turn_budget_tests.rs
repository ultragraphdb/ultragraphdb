/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_storage::{TURN_BYTES, TurnBudget};

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
fn test_small_frames_end_a_turn_too() {
    let mut budget = TurnBudget::default();
    let mut frames = 0usize;
    while !budget.is_spent() {
        budget.spend_frame(1);
        frames += 1;
    }
    assert!(frames > 1, "a turn serves several Scans");
    assert!(frames < TURN_BYTES / 1_024, "{frames} frames of one byte");
}

/// Pages holding nothing spend a turn, so a turn of emptied ranges ends.
#[test]
fn test_idle_pages_end_a_turn() {
    let mut budget = TurnBudget::default();
    let mut pages = 0usize;
    while !budget.is_spent() {
        budget.spend_idle_page();
        pages += 1;
    }
    assert!(pages > 1, "a page does not spend a turn whole");
}

/// A turn's last frame is left room enough to be worth its seek, however little is left.
#[test]
fn test_a_turns_last_frame_is_worth_its_seek() {
    let mut budget = TurnBudget::default();
    budget.spend_frame(TURN_BYTES - 1);
    assert!(!budget.is_spent());
    assert!(budget.fill_to() > 1);
}
