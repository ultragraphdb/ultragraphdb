/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_traits_store::{Entry, MAX_STORED_KEY_SIZE, MAX_USER_KEY_SIZE, WriteBatch};

#[test]
fn test_a_new_batch_is_empty() {
    let batch = WriteBatch::new();
    assert_eq!(batch, WriteBatch::default());
    assert!(batch.is_empty());
    assert_eq!((batch.len(), batch.applied_ts()), (0, None));
    assert!(batch.entries().is_empty() && batch.removed_rebuild_records().is_empty());
    assert_eq!(batch.check_key_sizes(), Ok(()));
}

#[test]
fn test_a_batch_keeps_its_entries_in_order_duplicates_included() {
    let entries = vec![
        Entry::put(b"b".to_vec(), 12, 0x04, vec![1]),
        Entry::deletion(b"a".to_vec(), 13),
        Entry::put(b"b".to_vec(), 12, 0x08, vec![2]),
    ];
    let mut batch = WriteBatch::new();
    entries.iter().cloned().for_each(|e| batch.push(e));
    assert!(!batch.is_empty());
    assert_eq!(batch.len(), 3);
    assert_eq!(batch.entries(), entries.as_slice());
    assert_eq!(batch.into_entries(), entries);
}

#[test]
fn test_a_batch_records_an_applied_timestamp_only_when_given_one() {
    let mut batch = WriteBatch::new();
    assert_eq!(batch.applied_ts(), None);
    batch.set_applied_ts(13);
    assert_eq!(batch.applied_ts(), Some(13));
    let mut zero = WriteBatch::new();
    zero.set_applied_ts(0);
    assert_eq!(zero.applied_ts(), Some(0));
    assert!(zero.is_empty(), "an applied timestamp is not an entry");
}

#[test]
fn test_check_key_sizes_refuses_the_first_oversized_key() {
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(b"ok".to_vec(), 1, 0, vec![]));
    batch.push(Entry::put(vec![7; MAX_USER_KEY_SIZE], 1, 0, vec![]));
    assert_eq!(batch.check_key_sizes(), Ok(()));
    // A stored-size refusal first, then a larger user-size one: the first is reported.
    batch.push(Entry::put(vec![0; 40_000], 1, 0, vec![]));
    batch.push(Entry::deletion(vec![1; MAX_USER_KEY_SIZE + 1], 1));
    let err = batch.check_key_sizes().unwrap_err();
    match err.0 {
        StoreErrorEnum::KeyTooLarge { size, limit, head } => {
            assert_eq!((size, limit), (80_010, MAX_STORED_KEY_SIZE));
            assert_eq!(head, [0x00, 0xFF].repeat(512));
        }
        other => panic!("expected KeyTooLarge, got {other:?}"),
    }
}

#[test]
fn test_check_key_sizes_checks_deletions_too() {
    let mut batch = WriteBatch::new();
    batch.push(Entry::deletion(vec![1; MAX_USER_KEY_SIZE + 1], 1));
    assert_eq!(
        batch.check_key_sizes(),
        Err(StoreError::KeyTooLarge(65_001, 65_000, vec![1; 1024]))
    );
}

#[test]
fn test_a_batch_removes_the_rebuild_records_it_names_in_order() {
    let mut batch = WriteBatch::new();
    batch.remove_rebuild_record(14);
    batch.remove_rebuild_record(12);
    batch.remove_rebuild_record(14);
    assert_eq!(batch.removed_rebuild_records(), [14, 12, 14]);
    assert!(batch.is_empty(), "a record's removal is not an entry");
    assert_eq!(batch.len(), 0);
}
