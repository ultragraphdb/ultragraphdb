/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_service::LatencyHistogram;

/// The most a quantile read from a bucket lies above the value it stands for: a sixteenth of the value's octave.
const ERROR_PERMILLE: u64 = 62;

/// A histogram holding nothing reads 0 everywhere.
#[test]
fn test_an_empty_histogram_reads_zero() {
    let histogram = LatencyHistogram::new();
    assert_eq!(histogram.count(), 0);
    assert_eq!(histogram.mean(), 0);
    assert_eq!(histogram.max(), 0);
    assert_eq!(histogram.quantile(500), 0);
    assert_eq!(histogram.quantile(1_000), 0);
}

/// Values below 16 have a bucket each, so their quantiles are exact.
#[test]
fn test_small_values_are_exact() {
    let mut histogram = LatencyHistogram::new();
    (1..=10).for_each(|us| histogram.record(us));
    assert_eq!(histogram.count(), 10);
    assert_eq!(histogram.quantile(500), 5);
    assert_eq!(histogram.quantile(900), 9);
    assert_eq!(histogram.quantile(1_000), 10);
    assert_eq!(histogram.quantile(0), 1, "the first value at the least");
    assert_eq!(histogram.mean(), 5, "55 over 10, rounded down");
}

/// A quantile of larger values lies at or above the true one, and within a sixteenth of its octave.
#[test]
fn test_a_quantile_lies_within_its_bucket() {
    let mut histogram = LatencyHistogram::new();
    let values: Vec<u64> = (0..1_000).map(|step| 100 + step * 97).collect();
    values.iter().for_each(|us| histogram.record(*us));
    for permille in [10, 500, 900, 990] {
        let rank = usize::try_from(permille).unwrap_or(usize::MAX);
        let truth = values.get(rank - 1).copied().unwrap_or(0);
        let read = histogram.quantile(permille);
        assert!(read >= truth, "{permille}‰: {read} under {truth}");
        assert!(
            read - truth <= truth * ERROR_PERMILLE / 1_000,
            "{permille}‰: {read} too far above {truth}"
        );
    }
    assert_eq!(histogram.max(), 100 + 999 * 97);
    assert_eq!(
        histogram.quantile(1_000),
        histogram.max(),
        "never past the largest"
    );
}

/// A value past the last bucket's range still counts, and the largest value is kept whole.
#[test]
fn test_a_value_past_the_range_counts() {
    let mut histogram = LatencyHistogram::new();
    histogram.record(u64::MAX);
    histogram.record(3);
    assert_eq!(histogram.count(), 2);
    assert_eq!(histogram.max(), u64::MAX);
    assert_eq!(histogram.quantile(500), 3);
    assert!(histogram.quantile(1_000) >= u64::from(u32::MAX) / 2);
}

/// A cleared histogram counts from nothing again.
#[test]
fn test_a_cleared_histogram_counts_again() {
    let mut histogram = LatencyHistogram::new();
    (0..100).for_each(|us| histogram.record(us * 1_000));
    histogram.clear();
    assert_eq!(histogram, LatencyHistogram::default());
    histogram.record(42);
    assert_eq!(histogram.quantile(500), 42);
}
