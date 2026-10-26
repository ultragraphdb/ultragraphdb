/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use ultragraphdb_tool_bench::{Clock, WallClock};

#[test]
fn test_the_wall_clock_counts_from_when_it_was_made() {
    let clock = WallClock::new();
    let before = clock.elapsed();
    std::thread::sleep(Duration::from_millis(20));
    let after = clock.elapsed();
    // A sleep lasts at least what it asks for; the clock never runs back.
    assert!(
        after >= before + Duration::from_millis(20),
        "{before:?} {after:?}"
    );
    assert!(
        WallClock::default().elapsed() < after,
        "a later clock starts later"
    );
}
