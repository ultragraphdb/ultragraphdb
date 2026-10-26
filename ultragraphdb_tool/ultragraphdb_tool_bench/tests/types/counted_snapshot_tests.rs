/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;
use std::sync::Arc;

use ultragraphdb_store_kv::{Durability, FjallStore};
use ultragraphdb_tool_bench::{ReadCounts, TimedStore};
use ultragraphdb_traits_store::{Entry, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

#[test]
fn test_reads_of_a_keys_versions_are_counted_and_those_that_found_none() {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let dir = root.join(format!("bench-counted-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let counts = Arc::new(ReadCounts::new());
    let store = TimedStore::new(
        FjallStore::open(&dir, Durability::Process).unwrap(),
        Arc::clone(&counts),
    );
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(b"a".to_vec(), 3, 0, b"v".to_vec()));
    batch.push(Entry::put(b"a".to_vec(), 4, 0, b"w".to_vec()));
    batch.put_versionless(b"k".to_vec(), b"kv".to_vec());
    store.apply(batch, PersistMode::Buffer).unwrap();
    let snapshot = store.snapshot(9).unwrap();
    assert_eq!(snapshot.versions(b"a").count(), 2);
    assert_eq!(snapshot.versions(b"b").count(), 0);
    assert_eq!(counts.get(), (0, 0, 2, 1));
    // A read of versions counts once it is made; empty only once it has been seen to yield nothing.
    let unread = snapshot.versions(b"c");
    assert_eq!(counts.get(), (0, 0, 3, 1));
    drop(unread);
    assert_eq!(counts.get(), (0, 0, 3, 1));
    // Other reads are not counted, and see what the snapshot beneath sees.
    let scanned: Vec<(Vec<u8>, u64)> = snapshot
        .scan(&Scan::prefix(b"a".to_vec()).all_versions())
        .map(|item| {
            let item = item.unwrap();
            (item.key().to_vec(), item.version())
        })
        .collect();
    assert_eq!(scanned, [(b"a".to_vec(), 4), (b"a".to_vec(), 3)]);
    assert_eq!(snapshot.scan(&Scan::prefix(b"z".to_vec())).count(), 0);
    assert_eq!(snapshot.get(b"a").unwrap().map(|i| i.version()), Some(4));
    assert!(snapshot.get(b"b").unwrap().is_none());
    let many: Vec<Option<u64>> = snapshot
        .get_many(&[b"b".as_slice(), b"a".as_slice()])
        .unwrap()
        .into_iter()
        .map(|item| item.map(|i| i.version()))
        .collect();
    assert_eq!(many, [None, Some(4)]);
    assert_eq!(
        snapshot.get_versionless(b"k").unwrap(),
        Some(b"kv".to_vec())
    );
    assert_eq!(snapshot.read_ts(), 9);
    assert_eq!(store.snapshot(3).unwrap().read_ts(), 3);
    assert_eq!(counts.get(), (0, 0, 3, 1));
    drop(snapshot);
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}
