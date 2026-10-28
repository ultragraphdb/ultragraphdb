/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_transactions_txn::TxnBuffers;

#[test]
fn test_new_buffers_hold_nothing_and_reserved_ones_their_capacity() {
    let empty = TxnBuffers::new();
    assert_eq!(empty.record_capacity(), 0);
    let reserved = TxnBuffers::with_capacity(1_000, 10, 2_000);
    let (bytes, keys) = reserved.read_set_capacity();
    assert!(bytes >= 1_000 && keys >= 40, "{bytes} {keys}");
    assert!(reserved.record_capacity() >= 2_000);
}
