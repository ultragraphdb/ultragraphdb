/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;
use std::sync::Arc;

use ultragraphdb_store_kv::{Durability, FjallStore};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_bench::{ReadCounts, TimedStore};
use ultragraphdb_traits_store::{
    Entry, PersistMode, RebuildRecord, Store, StoreSnapshot, WriteBatch,
};

#[test]
fn test_commit_write_calls_are_recorded_and_the_others_counted() -> TestResult {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let dir = root.join(format!("bench-timed-{}", std::process::id()));
    // A previous run's directory, if one is left.
    if let Err(e) = std::fs::remove_dir_all(&dir) {
        assert_eq!(
            e.kind(),
            std::io::ErrorKind::NotFound,
            "cannot clear {}: {e}",
            dir.display()
        );
    }
    let store = TimedStore::new(
        FjallStore::open(&dir, Durability::Process)?,
        Arc::new(ReadCounts::new()),
    );
    let mut commit = WriteBatch::new();
    commit.push(Entry::put(b"a".to_vec(), 3, 0, b"v".to_vec()));
    commit.push(Entry::put(b"b".to_vec(), 3, 0, b"v".to_vec()));
    commit.set_applied_ts(3);
    store.apply(commit, PersistMode::Buffer)?;
    let mut rollup = WriteBatch::new();
    rollup.push(Entry::put(b"a".to_vec(), 4, 0, b"w".to_vec()));
    store.apply(rollup, PersistMode::Buffer)?;
    // Commit batches are kept call by call; the others, rollup batches, are counted.
    let calls = store.take_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!((calls[0].entries(), calls[0].commit()), (2, true));
    assert!(calls[0].l0_tables() < 20 && calls[0].sealed_memtables() < 4);
    assert!(store.take_calls().is_empty(), "taken once");
    let (other_calls, other_nanos) = store.other_calls();
    assert_eq!(other_calls, 1);
    assert!(other_nanos > 0);
    assert_eq!(store.get_at(b"a", 9)?.map(|i| i.version()), Some(4));
    assert_eq!(store.inner().applied_ts(), 3);
    drop(store);
    std::fs::remove_dir_all(&dir).context("removing the test's directory")?;
    Ok(())
}

/// A fresh store in its own directory under the test's temporary root.
fn open(name: &str) -> TestResult<(TimedStore, PathBuf)> {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let dir = root.join(format!("bench-timed-{name}-{}", std::process::id()));
    // A previous run's directory, if one is left.
    if let Err(e) = std::fs::remove_dir_all(&dir) {
        assert_eq!(
            e.kind(),
            std::io::ErrorKind::NotFound,
            "cannot clear {}: {e}",
            dir.display()
        );
    }
    let store = TimedStore::new(
        FjallStore::open(&dir, Durability::Process)?,
        Arc::new(ReadCounts::new()),
    );
    Ok((store, dir))
}

/// Every stored version of `key`, newest first.
fn versions(store: &TimedStore, key: &[u8]) -> TestResult<Vec<u64>> {
    Ok(store
        .snapshot(u64::MAX)?
        .versions(key)
        .map(|item| item.map(|found| found.version()))
        .collect::<Result<_, _>>()?)
}

/// A key of namespace `ns`: a prefix byte, the namespace's eight big-endian bytes, then one byte more.
fn namespaced(ns: u64) -> Vec<u8> {
    let mut key = vec![0u8];
    key.extend_from_slice(&ns.to_be_bytes());
    key.push(b'k');
    key
}

#[test]
fn test_the_store_calls_reach_the_store_beneath() -> TestResult {
    let (store, dir) = open("calls")?;
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(b"a".to_vec(), 3, 0, b"v".to_vec()));
    batch.push(Entry::put(b"a".to_vec(), 4, 0, b"w".to_vec()).with_discard());
    batch.push(Entry::deletion(b"d".to_vec(), 2));
    batch.push(Entry::put(b"p1".to_vec(), 3, 0, b"v".to_vec()));
    batch.push(Entry::put(namespaced(5), 3, 0, b"v".to_vec()));
    batch.put_versionless(b"k".to_vec(), b"kv".to_vec());
    batch.set_applied_ts(4);
    store.apply(batch, PersistMode::Buffer)?;
    assert_eq!(store.applied_ts(), 4);
    assert_eq!(store.get_versionless(b"k")?, Some(b"kv".to_vec()));
    assert_eq!(
        store.snapshot(9)?.get_versionless(b"k")?,
        Some(b"kv".to_vec())
    );

    // A ban hides the namespace's keys from later snapshots.
    assert!(store.get_at(&namespaced(5), 9)?.is_some());
    store.ban_namespace(5)?;
    assert_eq!(store.get_at(&namespaced(5), 9)?, None);

    // A prefix drop removes every version under it and writes its entries.
    store.drop_prefixes(
        6,
        &[b"p".to_vec()],
        vec![Entry::put(b"s".to_vec(), 1, 0, b"x".to_vec())],
    )?;
    assert_eq!(versions(&store, b"p1")?, Vec::<u64>::new());
    assert_eq!(versions(&store, b"s")?, [1]);

    // Below the discard timestamp compaction keeps a's discard-flagged version only, and the sweep removes the
    // deletion at or below the tombstone horizon.
    store.set_discard_ts(5)?;
    assert_eq!(store.discard_ts(), 5);
    store.set_tombstone_horizon(5)?;
    assert_eq!(store.tombstone_horizon(), 5);
    assert_eq!(versions(&store, b"a")?, [4, 3]);
    store.compact()?;
    assert_eq!(versions(&store, b"a")?, [4]);
    // No read reports a deletion. Once the sweep has removed d's, a version written below it is the newest; while
    // it stands, it hides that version.
    store.sweep_deletions()?;
    let mut below = WriteBatch::new();
    below.push(Entry::put(b"d".to_vec(), 1, 0, b"v".to_vec()));
    store.apply(below, PersistMode::Buffer)?;
    assert_eq!(store.get_at(b"d", 9)?.map(|i| i.version()), Some(1));
    store.sync()?;

    // Dropping everything clears the keys, the versionless keys and the applied timestamp, and writes its entries.
    store.drop_all(vec![Entry::put(b"t".to_vec(), 1, 0, b"y".to_vec())])?;
    assert_eq!(store.applied_ts(), 0);
    assert_eq!(store.get_versionless(b"k")?, None);
    assert_eq!(versions(&store, b"a")?, Vec::<u64>::new());
    assert_eq!(versions(&store, b"t")?, [1]);
    drop(store);
    std::fs::remove_dir_all(&dir).context("removing the test's directory")?;
    Ok(())
}

#[test]
fn test_temporaries_and_rebuild_records_reach_the_store_beneath() -> TestResult {
    let (store, dir) = open("temporary")?;
    let temporary = store.create_temporary()?;
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(b"x".to_vec(), 2, 0, b"v".to_vec()));
    store.apply_temporary(&temporary, batch)?;
    let snapshot = store.temporary_snapshot(&temporary)?;
    assert_eq!(snapshot.get(b"x")?.map(|i| i.version()), Some(2));
    drop(snapshot);
    // The data never sees the temporary's keys.
    assert_eq!(store.get_at(b"x", u64::MAX)?, None);
    store.drop_temporary(temporary)?;

    let record = RebuildRecord::new(7, vec![b"p".to_vec()]);
    assert!(store.rebuild_records()?.is_empty());
    store.write_rebuild_record(&record)?;
    assert_eq!(store.rebuild_records()?, [record]);
    drop(store);
    std::fs::remove_dir_all(&dir).context("removing the test's directory")?;
    Ok(())
}
