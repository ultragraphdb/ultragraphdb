/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the tests share: a store directory under `TEST_TMPDIR`, removed when the test ends, and short ways to write
//! and read.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fjall::{Database, KeyspaceCreateOptions};
use ultragraphdb_store_kv::{DATA_KEYSPACE, Durability, FjallStore};
use ultragraphdb_traits_store::{Entry, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A directory of its own under Bazel's `TEST_TMPDIR`, or the system's temporary directory under Cargo.
#[derive(Debug)]
pub struct TestDir(PathBuf);

impl TestDir {
    pub fn new(name: &str) -> Self {
        let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = root.join(format!("kv-{name}-{}-{n}", std::process::id()));
        // A directory an earlier run left behind, which is usually absent.
        drop(std::fs::remove_dir_all(&dir));
        Self(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn open(&self) -> FjallStore {
        FjallStore::open(&self.0, Durability::Process).expect("the store opens")
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        // A directory left behind is litter in the temporary directory, not a failure of the test.
        drop(std::fs::remove_dir_all(&self.0));
    }
}

pub fn write(store: &FjallStore, entries: Vec<Entry>) {
    let mut batch = WriteBatch::new();
    entries.into_iter().for_each(|e| batch.push(e));
    store
        .apply(batch, PersistMode::Buffer)
        .expect("the batch is written");
}

pub fn put(key: &[u8], version: u64, value: &[u8]) -> Entry {
    Entry::put(key.to_vec(), version, 0x04, value.to_vec())
}

/// A key of namespace `ns`.
pub fn ns_key(ns: u64, tail: &[u8]) -> Vec<u8> {
    let mut key = vec![0x00];
    key.extend_from_slice(&ns.to_be_bytes());
    key.extend_from_slice(tail);
    key
}

pub fn versions(store: &FjallStore, key: &[u8], ts: u64) -> Vec<u64> {
    let snapshot = store.snapshot(ts).expect("readable");
    snapshot
        .versions(key)
        .map(|i| i.expect("read").version())
        .collect()
}

pub fn scanned(store: &FjallStore, scan: &Scan, ts: u64) -> Vec<(Vec<u8>, u64)> {
    let snapshot = store.snapshot(ts).expect("readable");
    snapshot
        .scan(scan)
        .map(|i| i.expect("read"))
        .map(|i| (i.key().to_vec(), i.version()))
        .collect()
}

/// The versions the store holds of `key`, newest first, with whether each is a deletion.
pub fn held(store: &FjallStore, key: &[u8]) -> Vec<(u64, bool)> {
    store
        .dump()
        .expect("dumps")
        .iter()
        .filter(|e| e.key() == key)
        .map(|e| (e.version(), e.is_deletion()))
        .collect()
}

/// Writes `value` raw at the stored key of `key` at `version` into the closed store in `dir`.
pub fn write_raw(dir: &TestDir, key: &[u8], version: u64, value: &[u8]) {
    write_raw_in(dir, DATA_KEYSPACE, &stored(key, version), value);
}

/// Writes `value` raw at `key` of `keyspace` into the closed store in `dir`, whatever the store would make of it.
pub fn write_raw_in(dir: &TestDir, keyspace: &str, key: &[u8], value: &[u8]) {
    let db = Database::builder(dir.path()).open().unwrap();
    let ks = db
        .keyspace(keyspace, KeyspaceCreateOptions::default)
        .unwrap();
    ks.insert(key.to_vec(), value.to_vec()).unwrap();
    db.persist(fjall::PersistMode::SyncAll).unwrap();
}

/// The stored key as the specification states it: every run of zero bytes as `0x00` and a code (`0xFF - k` for `k`
/// bytes before a non-zero byte, `0x80` for 126 bytes before more, `0x01 + j` for `j` bytes ending the key), the
/// complemented version after the end code.
pub fn stored(key: &[u8], version: u64) -> Vec<u8> {
    let mut out = Vec::new();
    let mut run = 0u8;
    for &b in key {
        match (b, run) {
            (0, 126) => {
                out.extend_from_slice(&[0x00, 0x80]);
                run = 1;
            }
            (0, _) => run += 1,
            (_, 0) => out.push(b),
            _ => {
                out.extend_from_slice(&[0x00, 0xFF - run, b]);
                run = 0;
            }
        }
    }
    out.extend_from_slice(&[0x00, 0x01 + run]);
    out.extend_from_slice(&(!version).to_be_bytes());
    out
}
