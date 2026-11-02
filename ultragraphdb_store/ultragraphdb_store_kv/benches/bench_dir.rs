/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the benches share: a store in a directory of its own under the system's temporary directory, removed when
//! the store is dropped, and the keys and batches they write.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_kv::{Durability, FjallStore};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A fjall store under the system's temporary directory, never the repository; its directory is removed after the
/// store closes.
pub struct BenchStore {
    // Dropped first: the database closes before its directory is removed.
    store: FjallStore,
    dir: PathBuf,
}

impl BenchStore {
    /// Opens a store named `name` with `durability` in a fresh directory.
    pub fn open(name: &str, durability: Durability) -> Result<Self, StoreError> {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "ultragraphdb-kv-bench-{name}-{}-{n}",
            std::process::id()
        ));
        // A directory an earlier run left behind, which is usually absent.
        drop(std::fs::remove_dir_all(&dir));
        let store = FjallStore::open(&dir, durability)?;
        Ok(Self { store, dir })
    }

    /// The store.
    pub fn store(&self) -> &FjallStore {
        &self.store
    }
}

impl Drop for BenchStore {
    fn drop(&mut self) {
        // A directory left behind in the temporary directory is litter, not a failure of the bench.
        drop(std::fs::remove_dir_all(&self.dir));
    }
}

/// `prefix` followed by `i`, big-endian, so that keys sort as their numbers.
pub fn key(prefix: &[u8], i: u64) -> Vec<u8> {
    let mut key = prefix.to_vec();
    key.extend_from_slice(&i.to_be_bytes());
    key
}

/// Writes `count` keys under `prefix`, each at `version` with a value of `value_len` bytes, in batches of 10,000.
pub fn fill(
    store: &FjallStore,
    prefix: &[u8],
    count: u64,
    version: u64,
    value_len: usize,
) -> Result<(), StoreError> {
    for chunk in (0..count).collect::<Vec<_>>().chunks(10_000) {
        let mut batch = WriteBatch::new();
        chunk
            .iter()
            .for_each(|&i| batch.push(Entry::put(key(prefix, i), version, 0, vec![7; value_len])));
        store.apply(batch, PersistMode::Buffer)?;
    }
    Ok(())
}

/// Runs the bench `name`, and on a failure of its setup reports the failure and ends the process with exit code 1, as
/// a bench that cannot measure has failed.
pub fn run(name: &str, bench: impl FnOnce() -> Result<(), StoreError>) {
    if let Err(e) = bench() {
        eprintln!("{name}: {e}");
        std::process::exit(1);
    }
}
