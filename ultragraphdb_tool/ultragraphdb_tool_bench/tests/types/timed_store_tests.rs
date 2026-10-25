/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;

use ultragraphdb_store_kv::{Durability, FjallStore};
use ultragraphdb_tool_bench::TimedStore;
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

#[test]
fn test_commit_write_calls_are_recorded_and_the_others_counted() {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let dir = root.join(format!("bench-timed-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = TimedStore::new(FjallStore::open(&dir, Durability::Process).unwrap());
    let mut commit = WriteBatch::new();
    commit.push(Entry::put(b"a".to_vec(), 3, 0, b"v".to_vec()));
    commit.push(Entry::put(b"b".to_vec(), 3, 0, b"v".to_vec()));
    commit.set_applied_ts(3);
    store.apply(commit, PersistMode::Buffer).unwrap();
    let mut rollup = WriteBatch::new();
    rollup.push(Entry::put(b"a".to_vec(), 4, 0, b"w".to_vec()));
    store.apply(rollup, PersistMode::Buffer).unwrap();
    // Commit batches are kept call by call; the others, rollup batches, are counted.
    let calls = store.take_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!((calls[0].entries(), calls[0].commit()), (2, true));
    assert!(calls[0].l0_tables() < 20 && calls[0].sealed_memtables() < 4);
    assert!(store.take_calls().is_empty(), "taken once");
    let (other_calls, other_nanos) = store.other_calls();
    assert_eq!(other_calls, 1);
    assert!(other_nanos > 0);
    assert_eq!(store.get_at(b"a", 9).unwrap().map(|i| i.version()), Some(4));
    assert_eq!(store.inner().applied_ts(), 3);
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}
