/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_transactions_commit::Sequencer;

#[test]
fn test_a_round_at_a_slow_clock_follows_the_previous_version_by_two() {
    // max(0 + 2, 2 × 0) = 2, then 4 and 6 within the round.
    let mut sequencer = Sequencer::new(0);
    assert_eq!(sequencer.previous(), 0);
    assert_eq!(sequencer.open_round(0), Some(2));
    assert_eq!(sequencer.take(), Some(2));
    assert_eq!(sequencer.take(), Some(4));
    assert_eq!(sequencer.take(), Some(6));
    assert_eq!(sequencer.previous(), 6);
}

#[test]
fn test_a_round_at_a_fast_clock_takes_twice_its_microseconds() {
    // max(6 + 2, 2 × 1,000) = 2,000.
    let mut sequencer = Sequencer::new(6);
    assert_eq!(sequencer.open_round(1_000), Some(2_000));
    assert_eq!(sequencer.take(), Some(2_000));
    assert_eq!(sequencer.take(), Some(2_002));
}

#[test]
fn test_a_clock_stepping_back_never_lowers_versions() {
    // A round at 10 s takes 20,000,000; one at 9 s takes max(20,000,000 + 2, 18,000,000).
    let mut sequencer = Sequencer::new(0);
    assert_eq!(sequencer.open_round(10_000_000), Some(20_000_000));
    assert_eq!(sequencer.take(), Some(20_000_000));
    assert_eq!(sequencer.open_round(9_000_000), Some(20_000_002));
    assert_eq!(sequencer.take(), Some(20_000_002));
}

#[test]
fn test_the_clock_is_read_once_per_round() {
    // A round opened at 5 µs gives 10, 12, 14 whatever the clock does during it; the next round at 6 µs takes
    // max(14 + 2, 12) = 16.
    let mut sequencer = Sequencer::new(0);
    assert_eq!(sequencer.open_round(5), Some(10));
    assert_eq!(sequencer.take(), Some(10));
    assert_eq!(sequencer.take(), Some(12));
    assert_eq!(sequencer.take(), Some(14));
    assert_eq!(sequencer.open_round(6), Some(16));
}

#[test]
fn test_an_unused_round_gives_no_version() {
    // A round whose transactions all abort takes none: the next round still follows 4.
    let mut sequencer = Sequencer::new(4);
    assert_eq!(sequencer.open_round(0), Some(6));
    assert_eq!(sequencer.open_round(0), Some(6));
    assert_eq!(sequencer.previous(), 4);
}

#[test]
fn test_an_odd_previous_version_is_followed_by_the_even_one_above() {
    // After a rollup's odd version 13, the next commit is 14.
    let mut sequencer = Sequencer::new(13);
    assert_eq!(sequencer.open_round(0), Some(14));
    assert_eq!(sequencer.take(), Some(14));
}

#[test]
fn test_no_version_before_a_round_is_opened() {
    let mut sequencer = Sequencer::new(8);
    assert_eq!(sequencer.take(), None);
    assert_eq!(sequencer.previous(), 8);
}

#[test]
fn test_versions_past_u64_max_are_exhausted() {
    assert_eq!(Sequencer::new(u64::MAX - 1).open_round(0), None);
    assert_eq!(Sequencer::new(u64::MAX).open_round(0), None);
    assert_eq!(Sequencer::new(0).open_round(u64::MAX / 2 + 1), None);
    // u64::MAX − 3 is even: u64::MAX − 1 follows it, and nothing follows that.
    let mut sequencer = Sequencer::new(u64::MAX - 3);
    assert_eq!(sequencer.open_round(0), Some(u64::MAX - 1));
    assert_eq!(sequencer.take(), Some(u64::MAX - 1));
    assert_eq!(sequencer.take(), None);
    assert_eq!(sequencer.previous(), u64::MAX - 1);
}

#[test]
fn test_an_exhausted_round_forgets_the_version_of_the_round_before() {
    // A round opened, then one whose clock overflows twice its microseconds: none, not the earlier round's 2.
    let mut sequencer = Sequencer::new(0);
    assert_eq!(sequencer.open_round(0), Some(2));
    assert_eq!(sequencer.open_round(u64::MAX / 2 + 1), None);
    assert_eq!(sequencer.take(), None);
}

#[test]
fn test_the_clock_and_the_previous_version_meeting_and_one_apart() {
    // After 8: at 5 µs both rules give 10, a tie; at 6 µs the clock's 12 wins; at 4 µs the previous version's 10 wins
    // over the clock's 8. Each is max(8 + 2, 2 × clock) by hand.
    for (now_us, first) in [(5u64, 10u64), (6, 12), (4, 10)] {
        let mut sequencer = Sequencer::new(8);
        assert_eq!(sequencer.open_round(now_us), Some(first), "at {now_us} µs");
        assert_eq!(sequencer.take(), Some(first));
        assert_eq!(sequencer.take(), Some(first + 2));
    }
}

#[test]
fn test_the_largest_clock_whose_double_fits_still_gives_a_version() {
    // 2 × (u64::MAX / 2) = u64::MAX − 1, the largest even u64; one microsecond more overflows (the test above).
    let mut sequencer = Sequencer::new(0);
    assert_eq!(sequencer.open_round(u64::MAX / 2), Some(u64::MAX - 1));
    assert_eq!(sequencer.take(), Some(u64::MAX - 1));
    assert_eq!(sequencer.take(), None, "nothing follows the largest even version");
}
