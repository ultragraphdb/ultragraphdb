/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The constants are Dgraph's, stored and compared: each value is read off Dgraph's source.

use ultragraphdb_store_posting::{
    BIT_COMPLETE_POSTING, BIT_DELTA_POSTING, BIT_EMPTY_POSTING, BIT_SCHEMA_POSTING, BLOCK_SIZE,
    DEL, HIGH_PRIORITY_DELTAS, MAX_LIST_SIZE, OVR, ROLLUP_BATCH, ROLLUP_DEDUP_SECS, ROLLUP_QUEUE,
    SET,
};

#[test]
fn test_the_operations_and_meta_bytes_are_dgraphs() {
    // posting/list.go:50-63.
    assert_eq!((SET, DEL, OVR), (0x01, 0x02, 0x03));
    assert_eq!(
        (
            BIT_SCHEMA_POSTING,
            BIT_DELTA_POSTING,
            BIT_COMPLETE_POSTING,
            BIT_EMPTY_POSTING
        ),
        (0x01, 0x04, 0x08, 0x10)
    );
}

#[test]
fn test_the_sizes_and_rollup_limits_are_dgraphs() {
    // posting/list.go:45 maxListSize = mb / 2, and 1570 blockSize = 256.
    assert_eq!(MAX_LIST_SIZE, 524_288);
    assert_eq!(BLOCK_SIZE, 256);
    // posting/mvcc.go:573 and 687, deltaCount > 500; 81 and 143, batches of 16 in channels of 16; 176, a key rolled
    // up within 10 seconds is skipped.
    assert_eq!(HIGH_PRIORITY_DELTAS, 500);
    assert_eq!(
        (ROLLUP_BATCH, ROLLUP_QUEUE, ROLLUP_DEDUP_SECS),
        (16, 16, 10)
    );
}
