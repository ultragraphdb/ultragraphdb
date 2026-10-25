/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_sim::SimClock;

#[test]
fn test_time_moves_only_when_advanced_and_clones_share_it() {
    let clock = SimClock::new();
    assert_eq!(clock.now(), 0);
    let clone = clock.clone();
    clone.advance(1_500);
    clock.advance(500);
    assert_eq!((clock.now(), clone.now()), (2_000, 2_000));
    clock.advance(0);
    assert_eq!(clock.now(), 2_000, "advancing by nothing leaves the time");
}

#[test]
fn test_separate_clocks_keep_separate_time() {
    let (a, b) = (SimClock::new(), SimClock::default());
    a.advance(7);
    assert_eq!((a.now(), b.now()), (7, 0));
}
