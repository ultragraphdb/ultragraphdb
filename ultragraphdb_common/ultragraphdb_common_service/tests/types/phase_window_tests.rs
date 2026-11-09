/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_service::{PHASES_QUIET_US, PHASES_WINDOW_US, PhaseWindow};

/// Each phase is counted in its own histogram, in the window's order.
#[test]
fn test_each_phase_has_its_histogram() {
    let mut window = PhaseWindow::<3>::new();
    window.record(1_000, 2_000, [100, 900, 1_000]);
    window.record(3_000, 3_500, [10, 490, 500]);
    assert_eq!(window.count(), 2);
    assert_eq!(window.figures(0), (10, 100, 55));
    // 500 lies in the bucket of 496 to 511, whose highest value a quantile reads.
    assert_eq!(window.figures(2), (511, 1_000, 750));
    assert_eq!(window.figures(3), (0, 0, 0), "past the last phase");
    assert!(window.histogram(3).is_none());
    assert_eq!(window.histogram(1).map(|h| h.max()), Some(900));
}

/// A window runs from its earliest beginning to its latest end, in whatever order its requests are counted.
#[test]
fn test_a_window_spans_its_requests() {
    let mut window = PhaseWindow::<1>::new();
    assert_eq!(window.window_us(), 0);
    assert_eq!(window.first_us(), None);
    window.record(5_000, 9_000, [4_000]);
    window.record(2_000, 6_000, [4_000]);
    assert_eq!(window.first_us(), Some(2_000));
    assert_eq!(window.window_us(), 7_000);
}

/// A window is due a quiet second after its last request ended, or a minute after its first began, and never while empty.
#[test]
fn test_a_window_is_due_after_a_pause_or_a_minute() {
    let mut window = PhaseWindow::<1>::new();
    assert!(!window.is_due(u64::MAX), "nothing counted");
    window.record(10_000, 11_000, [1_000]);
    assert!(!window.is_due(11_000 + PHASES_QUIET_US - 1));
    assert!(window.is_due(11_000 + PHASES_QUIET_US));
    for step in 1..=(PHASES_WINDOW_US / 500_000) {
        let began = 10_000 + step * 500_000;
        window.record(began, began + 1_000, [1_000]);
    }
    assert!(
        window.is_due(10_000 + PHASES_WINDOW_US),
        "a minute of requests without a pause"
    );
}

/// A cleared window counts from nothing.
#[test]
fn test_a_cleared_window_counts_again() {
    let mut window = PhaseWindow::<2>::new();
    window.record(10_000, 11_000, [1, 2]);
    window.clear();
    assert_eq!(window, PhaseWindow::default());
    assert!(!window.is_due(u64::MAX));
}
