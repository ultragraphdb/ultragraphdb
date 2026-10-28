/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use ultragraphdb_transactions_txn::{MAX_FRAME, TxnLimits};

#[test]
fn test_the_defaults_are_the_specs() {
    // 10 s and 16 s: the transactions spec's "Transactions have bounded lifetimes"; 1 MiB: the wire's maximum frame.
    let limits = TxnLimits::default();
    assert_eq!(limits.read_write(), Duration::from_secs(10));
    assert_eq!(limits.read_only(), Duration::from_secs(16));
    assert_eq!(MAX_FRAME, 1_048_576);
    assert_eq!(limits.max_write_set(), 1_048_576);
    assert_eq!(limits.max_read_set(), 1_048_576);
    assert_eq!(limits.objects_per_list(), 64);
}

#[test]
fn test_each_setting_is_set_alone() {
    let limits = TxnLimits::default()
        .with_read_write(Duration::from_millis(1_500))
        .with_read_only(Duration::from_secs(3))
        .with_max_write_set(4_096)
        .with_max_read_set(512)
        .with_objects_per_list(2);
    assert_eq!(limits.read_write(), Duration::from_millis(1_500));
    assert_eq!(limits.read_only(), Duration::from_secs(3));
    assert_eq!(limits.max_write_set(), 4_096);
    assert_eq!(limits.max_read_set(), 512);
    assert_eq!(limits.objects_per_list(), 2);
    let one = TxnLimits::default().with_max_read_set(7);
    assert_eq!(one.max_write_set(), 1_048_576);
    assert_eq!(one.read_write(), Duration::from_secs(10));
}

#[test]
fn test_byte_bounds_above_u32_are_capped() {
    let limits = TxnLimits::default()
        .with_max_write_set(usize::MAX)
        .with_max_read_set(u32::MAX as usize + 1);
    assert_eq!(limits.max_write_set(), u32::MAX as usize);
    assert_eq!(limits.max_read_set(), u32::MAX as usize);
}
