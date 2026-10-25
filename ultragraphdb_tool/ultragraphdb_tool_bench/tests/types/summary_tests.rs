/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bench::Summary;

#[test]
fn test_the_summary_takes_nearest_rank_percentiles() {
    let mut values: Vec<u64> = (1..=1_000).collect();
    values.reverse();
    let s = Summary::of(&mut values);
    assert_eq!(
        (s.p50(), s.p99(), s.max(), s.count()),
        (500, 990, 1_000, 1_000)
    );
    assert_eq!(s.mean(), 500.5);
}

#[test]
fn test_an_empty_summary_is_zero() {
    let s = Summary::of(&mut []);
    assert_eq!((s.p50(), s.p99(), s.max(), s.count()), (0, 0, 0, 0));
    assert_eq!(s.mean(), 0.0);
}
