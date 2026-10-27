/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_posting::Timestamps;

#[test]
fn test_open_reads_bound_the_discard_timestamp() {
    let mut ts = Timestamps::default();
    assert_eq!(ts.oldest_open_read(), None);
    assert_eq!(ts.discard_bound(50), 50);
    ts.open_read(7);
    ts.open_read(7);
    ts.open_read(9);
    assert_eq!(ts.discard_bound(50), 7);
    ts.close_read(7);
    assert_eq!(ts.oldest_open_read(), Some(7));
    ts.close_read(7);
    assert_eq!(ts.oldest_open_read(), Some(9));
    ts.close_read(3);
    assert_eq!(ts.discard_bound(4), 4);
}

/// The open reads are the whole state: two registries with the same reads are equal, whatever order they opened in.
#[test]
fn test_the_open_reads_are_the_state() {
    let (mut a, mut b) = (Timestamps::default(), Timestamps::default());
    a.open_read(3);
    a.open_read(u64::MAX);
    b.open_read(u64::MAX);
    b.open_read(3);
    assert_eq!(a, b);
    a.close_read(u64::MAX);
    assert_ne!(a, b);
    assert_eq!(a.discard_bound(u64::MAX), 3);
}
