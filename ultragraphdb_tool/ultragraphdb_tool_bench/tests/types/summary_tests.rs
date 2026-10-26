/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bench::Summary;

/// The count, p50, p99 and max of `values`.
fn ranks(values: &mut [u64]) -> (usize, u64, u64, u64) {
    let s = Summary::of(values);
    (s.count(), s.p50(), s.p99(), s.max())
}

// Every expectation below is the nearest rank by hand: the q-th percentile of n sorted samples is the sample at
// rank ceil(q * n), counted from 1.

#[test]
fn test_the_summary_takes_nearest_rank_percentiles() {
    let mut values: Vec<u64> = (1..=1_000).collect();
    values.reverse();
    let s = Summary::of(&mut values);
    // ceil(0.5 * 1000) = 500, ceil(0.99 * 1000) = 990; the mean of 1..=1000 is 1001 / 2.
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

#[test]
fn test_a_single_sample_is_every_percentile() {
    // ceil(0.5 * 1) = ceil(0.99 * 1) = 1.
    let mut one = [42];
    assert_eq!(ranks(&mut one), (1, 42, 42, 42));
    assert_eq!(Summary::of(&mut [42]).mean(), 42.0);
}

#[test]
fn test_small_counts_take_the_rank_rounded_up() {
    // n = 2: ranks 1 and 2. n = 3: ranks 2 and 3. n = 10: ranks 5 and 10.
    assert_eq!(ranks(&mut [20, 10]), (2, 10, 20, 20));
    assert_eq!(ranks(&mut [30, 10, 20]), (3, 20, 30, 30));
    let mut ten: Vec<u64> = (1..=10).map(|v| v * 10).collect();
    assert_eq!(ranks(&mut ten), (10, 50, 100, 100));
}

#[test]
fn test_p99_leaves_the_maximum_once_there_are_more_than_100_samples() {
    // n = 100: ceil(99) = 99, below the maximum. n = 101: ceil(50.5) = 51 and ceil(99.99) = 100.
    let mut hundred: Vec<u64> = (1..=100).collect();
    assert_eq!(ranks(&mut hundred), (100, 50, 99, 100));
    let mut hundred_one: Vec<u64> = (1..=101).rev().collect();
    assert_eq!(ranks(&mut hundred_one), (101, 51, 100, 101));
    // n = 200: ceil(100) = 100, ceil(198) = 198.
    let mut two_hundred: Vec<u64> = (1..=200).collect();
    assert_eq!(ranks(&mut two_hundred), (200, 100, 198, 200));
}

#[test]
fn test_tied_samples_keep_their_value_and_count() {
    let mut tied = [7, 7, 7, 7];
    assert_eq!(ranks(&mut tied), (4, 7, 7, 7));
    // Ties straddling the median: sorted 1, 5, 5, 5, 9; rank 3 is a 5, rank 5 the 9.
    let mut straddling = [5, 9, 5, 1, 5];
    assert_eq!(ranks(&mut straddling), (5, 5, 9, 9));
    // (1 + 5 + 5 + 5 + 9) / 5 = 5.
    assert_eq!(Summary::of(&mut [5, 9, 5, 1, 5]).mean(), 5.0);
}

#[test]
fn test_the_samples_are_left_sorted() {
    let mut values = [3, 1, 2];
    let _ = Summary::of(&mut values);
    assert_eq!(values, [1, 2, 3]);
}

#[test]
fn test_the_mean_is_fractional_and_does_not_overflow() {
    // (1 + 2) / 2 = 1.5.
    assert_eq!(Summary::of(&mut [1, 2]).mean(), 1.5);
    // Two samples of u64::MAX sum past u64; their mean is u64::MAX as an f64, 2^64.
    let mut max = [u64::MAX, u64::MAX];
    let s = Summary::of(&mut max);
    assert_eq!(s.mean(), 18_446_744_073_709_551_616.0);
    assert_eq!((s.p50(), s.max()), (u64::MAX, u64::MAX));
}
