/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_apply::ReadCounts;

#[test]
fn test_each_count_reads_back_as_given() {
    let counts = ReadCounts::new(11, 3, 5, 7, 2);
    assert_eq!(counts.key_bytes(), 11);
    assert_eq!(counts.lists(), 3);
    assert_eq!(counts.statements(), 5);
    assert_eq!(counts.ranges(), 7);
    assert_eq!(counts.absent(), 2);
}

#[test]
fn test_the_default_counts_nothing() {
    assert_eq!(ReadCounts::default(), ReadCounts::new(0, 0, 0, 0, 0));
}
