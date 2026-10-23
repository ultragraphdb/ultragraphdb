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
}
