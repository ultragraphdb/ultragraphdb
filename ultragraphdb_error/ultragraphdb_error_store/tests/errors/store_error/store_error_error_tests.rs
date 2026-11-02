/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_error_store::StoreError;

#[test]
fn test_is_a_std_error_without_a_source() {
    assert!(StoreError::poisoned().source().is_none());
}

#[test]
fn test_every_variant_is_a_boxed_std_error_with_its_text_and_no_source() {
    let cases: [(StoreError, &str); 9] = [
        (StoreError::ts_too_old(), "Transaction is too old"),
        (
            StoreError::key_too_large(2, 1, vec![0x41]),
            "Key with size 2 exceeded 1 limit. Key:\n\
             00000000  41                                                |A|\n",
        ),
        (
            StoreError::read_failed(vec![0x01], "r".into()),
            "Read of key 01 failed: r",
        ),
        (
            StoreError::corrupted(vec![0x02], "c".into()),
            "Corrupted value of key 02: c",
        ),
        (
            StoreError::write_failed("w".into()),
            "Write to the store failed: w",
        ),
        (
            StoreError::poisoned(),
            "The store is poisoned by an earlier failed write and must be reopened",
        ),
        (
            StoreError::open_failed("o".into()),
            "Opening the store failed: o",
        ),
        (
            StoreError::unsupported("temporary keyspaces"),
            "The store does not support temporary keyspaces",
        ),
        (
            StoreError::unknown_temporary(3),
            "Temporary keyspace 3 does not exist: it was dropped, or the store was reopened since it was created",
        ),
    ];
    for (error, want) in cases {
        let boxed: Box<dyn Error + Send + Sync> = Box::new(error);
        assert_eq!(boxed.to_string(), want);
        assert!(boxed.source().is_none(), "{want}");
    }
}
