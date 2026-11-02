/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Temporary keyspaces: fjall keyspaces of the store's database beside the data, outside its versions, bans and
//! garbage collection, dropped at open when a process left them.

use fjall::Database;
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_kv::TEMPORARY_KEYSPACE_PREFIX;
use ultragraphdb_traits_store::{
    Entry, MAX_USER_KEY_SIZE, RebuildRecord, Scan, Store, StoreSnapshot, Temporary, WriteBatch,
};

use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_test::{TestDir, ns_key, put, write};

fn batch(entries: Vec<Entry>) -> WriteBatch {
    let mut batch = WriteBatch::new();
    entries.into_iter().for_each(|e| batch.push(e));
    batch
}

/// The temporary keyspaces fjall holds in the closed store of `dir`.
fn temporary_keyspaces(dir: &TestDir) -> TestResult<Vec<String>> {
    let db = Database::builder(dir.path())
        .open()
        .context("fjall opens")?;
    Ok(db
        .list_keyspace_names()
        .iter()
        .filter(|n| n.starts_with(TEMPORARY_KEYSPACE_PREFIX))
        .map(|n| n.to_string())
        .collect())
}

#[test]
fn test_a_temporary_keyspace_holds_its_versions_apart_from_the_data() -> TestResult {
    let dir = TestDir::new("temp-apart");
    let s = dir.open()?;
    let k = ns_key(0, b"k");
    write(&s, vec![put(&k, 3, b"data")])?;
    let t = s.create_temporary()?;
    s.apply_temporary(
        &t,
        batch(vec![
            put(&k, 1, b"one"),
            put(&k, 2, b"two"),
            put(&ns_key(0, b"m"), 9, b"m"),
            Entry::deletion(ns_key(0, b"gone"), 4),
        ]),
    )?;
    let snapshot = s.temporary_snapshot(&t)?;
    assert_eq!(snapshot.read_ts(), u64::MAX);
    let versions: Vec<u64> = snapshot
        .versions(&k)
        .map(|i| i.map(|i| i.version()))
        .collect::<Result<_, _>>()?;
    assert_eq!(versions, [2, 1]);
    let keys: Vec<Vec<u8>> = snapshot
        .scan(&Scan::prefix(vec![0x00]))
        .map(|i| i.map(|i| i.key().to_vec()))
        .collect::<Result<_, _>>()?;
    assert_eq!(keys, [k.clone(), ns_key(0, b"m")]);
    let dumped: Vec<u64> = s.dump()?.iter().map(Entry::version).collect();
    assert_eq!(dumped, [3], "the data holds none of it");
    assert_eq!(s.get_at(&ns_key(0, b"m"), u64::MAX)?, None);
    Ok(())
}

#[test]
fn test_a_temporary_snapshot_holds_no_discard_timestamp_back() -> TestResult {
    let dir = TestDir::new("temp-discard");
    let s = dir.open()?;
    let t = s.create_temporary()?;
    let _snapshot = s.temporary_snapshot(&t)?;
    s.set_discard_ts(7)?;
    assert_eq!(s.discard_ts(), 7);
    Ok(())
}

#[test]
fn test_a_temporary_keyspace_keeps_entries_of_a_banned_namespace() -> TestResult {
    let dir = TestDir::new("temp-banned");
    let s = dir.open()?;
    s.ban_namespace(3)?;
    let t = s.create_temporary()?;
    let key = ns_key(3, b"a");
    s.apply_temporary(&t, batch(vec![put(&key, 1, b"kept")]))?;
    assert_eq!(
        s.temporary_snapshot(&t)?.get(&key)?.map(|i| i.into_value()),
        Some(b"kept".to_vec())
    );
    Ok(())
}

#[test]
fn test_an_oversized_key_refuses_the_temporary_batch_whole() -> TestResult {
    let dir = TestDir::new("temp-oversized");
    let s = dir.open()?;
    let t = s.create_temporary()?;
    let k = ns_key(0, b"k");
    let refused = s
        .apply_temporary(
            &t,
            batch(vec![
                put(&k, 1, b""),
                put(&vec![1; MAX_USER_KEY_SIZE + 1], 1, b""),
            ]),
        )
        .err()
        .context("the expected error")?;
    // Badger's exceedsSize dumps the first 1 << 10 bytes of the user key.
    assert_eq!(
        refused.0,
        StoreErrorEnum::KeyTooLarge {
            size: 65_001,
            limit: MAX_USER_KEY_SIZE,
            head: vec![1; 1024],
        }
    );
    assert!(s.temporary_snapshot(&t)?.get(&k)?.is_none());
    // The versionless writes a temporary batch ignores are still held to the size limits: `WriteBatch::check_key_sizes`
    // covers them, and the simulated store refuses the same batch.
    let mut b = batch(vec![put(&k, 1, b"")]);
    b.put_versionless(vec![2; MAX_USER_KEY_SIZE + 1], Vec::new());
    assert_eq!(
        s.apply_temporary(&t, b),
        Err(StoreError::key_too_large(
            65_001,
            MAX_USER_KEY_SIZE,
            vec![2; 1024]
        ))
    );
    assert!(s.temporary_snapshot(&t)?.get(&k)?.is_none());
    assert!(!s.is_poisoned());
    Ok(())
}

#[test]
fn test_a_dropped_temporary_keyspace_is_gone() -> TestResult {
    let dir = TestDir::new("temp-dropped");
    let s = dir.open()?;
    let t = s.create_temporary()?;
    let id = t.id();
    s.apply_temporary(&t, batch(vec![put(&ns_key(0, b"k"), 1, b"")]))?;
    s.drop_temporary(t)?;
    let again = Temporary::new(id);
    assert_eq!(
        s.apply_temporary(&again, WriteBatch::new()),
        Err(StoreError::unknown_temporary(id))
    );
    assert_eq!(
        s.temporary_snapshot(&again).err(),
        Some(StoreError::unknown_temporary(id))
    );
    assert_eq!(
        s.drop_temporary(Temporary::new(id)),
        Err(StoreError::unknown_temporary(id)),
        "a keyspace is dropped once"
    );
    assert_ne!(s.create_temporary()?.id(), id, "a number is not reused");
    Ok(())
}

#[test]
fn test_a_temporary_keyspace_left_by_a_process_is_dropped_at_open() -> TestResult {
    let dir = TestDir::new("temp-left");
    let s = dir.open()?;
    let left = s.create_temporary()?;
    s.apply_temporary(&left, batch(vec![put(&ns_key(0, b"k"), 1, b"left")]))?;
    s.sync()?;
    drop(s);
    assert_eq!(
        temporary_keyspaces(&dir)?.len(),
        1,
        "the closed store holds it"
    );
    let reopened = dir.open()?;
    let t = reopened.create_temporary()?;
    assert!(
        reopened
            .temporary_snapshot(&t)?
            .get(&ns_key(0, b"k"))?
            .is_none(),
        "a new keyspace starts empty"
    );
    reopened.drop_temporary(t)?;
    drop(reopened);
    assert!(temporary_keyspaces(&dir)?.is_empty());
    Ok(())
}

#[test]
fn test_a_temporary_batch_ignores_its_applied_timestamp_and_rebuild_records() -> TestResult {
    let dir = TestDir::new("temp-ignores");
    let s = dir.open()?;
    let record = RebuildRecord::new(4, vec![ns_key(0, b"idx")]);
    s.write_rebuild_record(&record)?;
    let t = s.create_temporary()?;
    let mut b = batch(vec![put(&ns_key(0, b"k"), 1, b"")]);
    b.set_applied_ts(50);
    b.remove_rebuild_record(4);
    s.apply_temporary(&t, b)?;
    assert_eq!(s.applied_ts(), 0);
    assert_eq!(s.rebuild_records()?, [record]);
    assert_eq!(
        s.temporary_snapshot(&t)?
            .get(&ns_key(0, b"k"))?
            .map(|i| i.version()),
        Some(1)
    );
    Ok(())
}
