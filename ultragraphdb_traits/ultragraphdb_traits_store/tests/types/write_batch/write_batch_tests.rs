/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreErrorEnum;
use ultragraphdb_traits_store::{Entry, MAX_USER_KEY_SIZE, WriteBatch};

#[test]
fn test_a_batch_keeps_its_entries_in_order() {
    let mut batch = WriteBatch::new();
    assert!(batch.is_empty());
    batch.push(Entry::put(b"a".to_vec(), 12, 0x04, vec![1]));
    batch.push(Entry::put(b"b".to_vec(), 13, 0x04, vec![2]));
    assert_eq!(batch.len(), 2);
    let versions: Vec<u64> = batch.entries().iter().map(Entry::version).collect();
    assert_eq!(versions, vec![12, 13]);
    assert_eq!(batch.into_entries().len(), 2);
}

#[test]
fn test_a_batch_records_an_applied_timestamp_only_when_given_one() {
    let mut batch = WriteBatch::new();
    assert_eq!(batch.applied_ts(), None);
    batch.set_applied_ts(13);
    assert_eq!(batch.applied_ts(), Some(13));
}

#[test]
fn test_check_key_sizes_refuses_the_first_oversized_key() {
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(b"ok".to_vec(), 1, 0, vec![]));
    assert!(batch.check_key_sizes().is_ok());
    batch.push(Entry::deletion(vec![1; MAX_USER_KEY_SIZE + 1], 1));
    let err = batch.check_key_sizes().unwrap_err();
    assert!(matches!(
        err.0,
        StoreErrorEnum::KeyTooLarge {
            size: 65_001,
            limit: 65_000,
            ..
        }
    ));
}
