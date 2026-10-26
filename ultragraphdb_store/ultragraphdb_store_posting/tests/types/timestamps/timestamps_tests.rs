/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_posting::Timestamps;

#[test]
fn test_the_counter_starts_two_above_the_applied_timestamp() {
    let mut ts = Timestamps::new(10);
    assert_eq!(ts.peek(), 12);
    assert_eq!(ts.fresh(), 12);
    assert_eq!(ts.fresh(), 13);
    ts.observe(20);
    assert_eq!(ts.fresh(), 21);
    ts.observe(5);
    assert_eq!(ts.peek(), 22);
}

#[test]
fn test_open_reads_bound_the_discard_timestamp() {
    let mut ts = Timestamps::new(0);
    assert_eq!(ts.oldest_open_read(), None);
    assert_eq!(ts.discard_bound(50), 50);
    ts.open_read(7);
    ts.open_read(7);
    ts.open_read(9);
    assert_eq!(ts.peek(), 10, "a read moves the counter past it");
    assert_eq!(ts.discard_bound(50), 7);
    ts.close_read(7);
    assert_eq!(ts.oldest_open_read(), Some(7));
    ts.close_read(7);
    assert_eq!(ts.oldest_open_read(), Some(9));
    ts.close_read(3);
    assert_eq!(ts.discard_bound(4), 4);
}

#[test]
fn test_a_read_of_everything_leaves_the_counter_where_it_is() {
    let mut ts = Timestamps::new(4);
    ts.open_read(u64::MAX);
    assert_eq!(ts.fresh(), 6, "a read at u64::MAX moves no commit above it");
    ts.close_read(u64::MAX);
    ts.observe(u64::MAX - 2);
    assert_eq!(
        ts.fresh(),
        u64::MAX - 1,
        "any other read moves the counter past it"
    );
}

#[test]
fn test_a_counter_at_the_top_of_the_range_saturates() {
    assert_eq!(Timestamps::new(u64::MAX - 1).peek(), u64::MAX);
    assert_eq!(Timestamps::new(u64::MAX).peek(), u64::MAX);
    assert_eq!(Timestamps::default().peek(), 0);
}
