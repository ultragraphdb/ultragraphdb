/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_traits_store::{
    Entry, MAX_STORED_KEY_SIZE, MAX_USER_KEY_SIZE, VersionlessEntry, WriteBatch,
};

#[test]
fn test_a_new_batch_is_empty() {
    let batch = WriteBatch::new();
    assert_eq!(batch, WriteBatch::default());
    assert!(batch.is_empty());
    assert_eq!((batch.len(), batch.applied_ts()), (0, None));
    assert!(batch.entries().is_empty() && batch.removed_rebuild_records().is_empty());
    assert!(batch.versionless().is_empty());
    assert_eq!(batch.check_key_sizes(), Ok(()));
}

#[test]
fn test_a_batch_keeps_its_versionless_writes_in_order_beside_its_entries() {
    let mut batch = WriteBatch::new();
    batch.put_versionless(b"t".to_vec(), vec![1]);
    assert!(!batch.is_empty(), "a versionless write alone is a write");
    assert_eq!(batch.len(), 0, "len counts versioned entries");
    batch.push(Entry::put(b"k".to_vec(), 3, 0x04, vec![2]));
    batch.remove_versionless(b"u".to_vec());
    batch.put_versionless(b"t".to_vec(), vec![3]);
    let versionless = vec![
        VersionlessEntry::put(b"t".to_vec(), vec![1]),
        VersionlessEntry::removal(b"u".to_vec()),
        VersionlessEntry::put(b"t".to_vec(), vec![3]),
    ];
    assert_eq!(batch.versionless(), versionless.as_slice());
    assert_eq!(batch.len(), 1);
    let (entries, parts) = batch.clone().into_parts();
    assert_eq!(entries, vec![Entry::put(b"k".to_vec(), 3, 0x04, vec![2])]);
    assert_eq!(parts, versionless);
    assert_eq!(batch.into_entries(), entries);
}

#[test]
fn test_check_key_sizes_covers_versionless_keys() {
    // 65,001 = MAX_USER_KEY_SIZE + 1 bytes against Badger's 65,000; the head is the first 1,024 bytes.
    let mut put = WriteBatch::new();
    put.push(Entry::put(b"ok".to_vec(), 1, 0, vec![]));
    put.put_versionless(vec![7; MAX_USER_KEY_SIZE + 1], vec![]);
    assert_eq!(
        put.check_key_sizes(),
        Err(StoreError::KeyTooLarge(65_001, 65_000, vec![7; 1024]))
    );
    // A removal names its key as a put does: 40,000 zero bytes are 80,010 stored (40,000 escapes, 2 + 8).
    let mut removal = WriteBatch::new();
    removal.remove_versionless(vec![0; 40_000]);
    assert_eq!(
        removal.check_key_sizes(),
        Err(StoreError::KeyTooLarge(
            80_010,
            65_535,
            [0x00, 0xFF].repeat(512)
        ))
    );
    // Versionless keys at both limits are accepted.
    let mut within = WriteBatch::new();
    within.put_versionless(vec![7; MAX_USER_KEY_SIZE], vec![]);
    within.remove_versionless([vec![0; 32_762], vec![1]].concat());
    assert_eq!(within.check_key_sizes(), Ok(()));
}

#[test]
fn test_check_key_sizes_reports_an_entry_before_a_versionless_key() {
    // The versionless write is named first and the entry second; the entry's refusal is reported, so a store
    // reports the same key whatever order the caller built its batch in.
    let mut batch = WriteBatch::new();
    batch.put_versionless(vec![9; MAX_USER_KEY_SIZE + 1], vec![]);
    batch.push(Entry::deletion(vec![0; 40_000], 1));
    assert_eq!(
        batch.check_key_sizes(),
        Err(StoreError::KeyTooLarge(
            80_010,
            65_535,
            [0x00, 0xFF].repeat(512)
        ))
    );
}

#[test]
fn test_check_key_sizes_refuses_an_empty_versionless_key_put_or_removed() {
    // Badger's ErrEmptyKey, "Key cannot be empty" (`txn.go:360`), for a put and for a removal alike.
    let mut put = WriteBatch::new();
    put.push(Entry::put(b"ok".to_vec(), 1, 0, vec![]));
    put.put_versionless(Vec::new(), b"v".to_vec());
    assert_eq!(put.check_key_sizes(), Err(StoreError::EmptyKey()));
    let mut removal = WriteBatch::new();
    removal.remove_versionless(Vec::new());
    assert_eq!(removal.check_key_sizes(), Err(StoreError::EmptyKey()));
    // The first refused key is reported: the empty one before an oversized one named after it, and an oversized
    // entry before both, entries being checked first.
    let mut first = WriteBatch::new();
    first.remove_versionless(Vec::new());
    first.put_versionless(vec![7; MAX_USER_KEY_SIZE + 1], vec![]);
    assert_eq!(first.check_key_sizes(), Err(StoreError::EmptyKey()));
    first.push(Entry::put(vec![8; MAX_USER_KEY_SIZE + 1], 1, 0, vec![]));
    assert_eq!(
        first.check_key_sizes(),
        Err(StoreError::KeyTooLarge(65_001, 65_000, vec![8; 1024]))
    );
    // A one-byte versionless key, and an empty versioned key, are accepted.
    let mut accepted = WriteBatch::new();
    accepted.put_versionless(vec![0], vec![]);
    accepted.push(Entry::put(Vec::new(), 1, 0, vec![]));
    accepted.push(Entry::deletion(Vec::new(), 2));
    assert_eq!(accepted.check_key_sizes(), Ok(()));
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
