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
