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
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_store::{Entry, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A directory of its own under Bazel's `TEST_TMPDIR`, or the system's temporary directory under Cargo.
#[derive(Debug)]
pub struct TestDir(PathBuf);

impl TestDir {
    /// A fresh directory for the test `name`, empty: one an earlier run left behind is removed.
    pub fn new(name: &str) -> Self {
        let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = root.join(format!("kv-{name}-{}-{n}", std::process::id()));
        // A directory an earlier run left behind, which is usually absent.
        drop(std::fs::remove_dir_all(&dir));
        Self(dir)
    }

    /// The directory's path.
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// The fjall store in the directory, opened with process durability.
    ///
    /// # Errors
    ///
    /// A [`TestError`](ultragraphdb_tool_bdd::TestError) when the store does not open.
    pub fn open(&self) -> TestResult<FjallStore> {
        FjallStore::open(&self.0, Durability::Process).context("the store opens")
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        // A directory left behind is litter in the temporary directory, not a failure of the test.
        drop(std::fs::remove_dir_all(&self.0));
    }
}

/// Writes `entries` to `store` as one batch, buffered.
///
/// # Errors
///
/// A [`TestError`](ultragraphdb_tool_bdd::TestError) when the store refuses the batch.
pub fn write(store: &FjallStore, entries: Vec<Entry>) -> TestResult {
    let mut batch = WriteBatch::new();
    entries.into_iter().for_each(|e| batch.push(e));
    store
        .apply(batch, PersistMode::Buffer)
        .context("the batch is written")
}

/// `value` as version `version` of `key`, with Dgraph's delta meta byte.
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

/// The versions of `key` a snapshot of `store` at `ts` reads, newest first.
///
/// # Errors
///
/// A [`TestError`](ultragraphdb_tool_bdd::TestError) when the snapshot does not open or a read fails.
pub fn versions(store: &FjallStore, key: &[u8], ts: u64) -> TestResult<Vec<u64>> {
    let snapshot = store.snapshot(ts).context("readable")?;
    snapshot
        .versions(key)
        .map(|i| i.map(|i| i.version()))
        .collect::<Result<_, _>>()
        .context("read")
}

/// The keys and versions `scan` reports from a snapshot of `store` at `ts`, in its order.
///
/// # Errors
///
/// A [`TestError`](ultragraphdb_tool_bdd::TestError) when the snapshot does not open or a read fails.
pub fn scanned(store: &FjallStore, scan: &Scan, ts: u64) -> TestResult<Vec<(Vec<u8>, u64)>> {
    let snapshot = store.snapshot(ts).context("readable")?;
    snapshot
        .scan(scan)
        .map(|i| i.map(|i| (i.key().to_vec(), i.version())))
        .collect::<Result<_, _>>()
        .context("read")
}

/// The versions the store holds of `key`, newest first, with whether each is a deletion.
///
/// # Errors
///
/// A [`TestError`](ultragraphdb_tool_bdd::TestError) when the store does not dump.
pub fn held(store: &FjallStore, key: &[u8]) -> TestResult<Vec<(u64, bool)>> {
    Ok(store
        .dump()
        .context("dumps")?
        .iter()
        .filter(|e| e.key() == key)
        .map(|e| (e.version(), e.is_deletion()))
        .collect())
}

/// Writes `value` raw at the stored key of `key` at `version` into the closed store in `dir`.
///
/// # Errors
///
/// A [`TestError`](ultragraphdb_tool_bdd::TestError) when fjall fails to open, write or persist.
pub fn write_raw(dir: &TestDir, key: &[u8], version: u64, value: &[u8]) -> TestResult {
    write_raw_in(dir, DATA_KEYSPACE, &stored(key, version), value)
}

/// Writes `value` raw at `key` of `keyspace` into the closed store in `dir`, whatever the store would make of it.
///
/// # Errors
///
/// A [`TestError`](ultragraphdb_tool_bdd::TestError) when fjall fails to open, write or persist.
pub fn write_raw_in(dir: &TestDir, keyspace: &str, key: &[u8], value: &[u8]) -> TestResult {
    let db = Database::builder(dir.path())
        .open()
        .context("fjall opens")?;
    let ks = db
        .keyspace(keyspace, KeyspaceCreateOptions::default)
        .context("the keyspace opens")?;
    ks.insert(key.to_vec(), value.to_vec())
        .context("the raw value is written")?;
    db.persist(fjall::PersistMode::SyncAll)
        .context("fjall persists")
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
