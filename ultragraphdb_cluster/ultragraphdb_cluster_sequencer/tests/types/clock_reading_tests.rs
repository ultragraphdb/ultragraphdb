/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_sequencer::ClockReading;

#[test]
fn test_a_reading_keeps_its_two_times_apart() {
    let reading = ClockReading::new(5_000, 1_700_000_000_000_000);
    assert_eq!(reading.monotonic_us(), 5_000);
    assert_eq!(reading.wall_us(), 1_700_000_000_000_000);
    let extremes = ClockReading::new(u64::MAX, 0);
    assert_eq!((extremes.monotonic_us(), extremes.wall_us()), (u64::MAX, 0));
}
