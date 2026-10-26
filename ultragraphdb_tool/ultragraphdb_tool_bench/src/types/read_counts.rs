/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::atomic::{AtomicU64, Ordering};

/// What the run's lookups and list reads found: lookups of the posting cache, by reads and by commits, and the entries
/// they found; the store's reads of a key's versions, by cache misses and rollups, and those that found none. Shared by
/// the cache and the store the run opens.
#[derive(Debug, Default)]
pub struct ReadCounts {
    lookups: AtomicU64,
    hits: AtomicU64,
    store_reads: AtomicU64,
    empty_reads: AtomicU64,
}

impl ReadCounts {
    pub fn new() -> Self {
        Self::default()
    }

    /// A cache lookup, and whether it found an entry.
    pub fn lookup(&self, hit: bool) {
        self.lookups.fetch_add(1, Ordering::Relaxed);
        if hit {
            self.hits.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// A read of a key's versions from the store.
    pub fn store_read(&self) {
        self.store_reads.fetch_add(1, Ordering::Relaxed);
    }

    /// A read of a key's versions that found none.
    pub fn empty_read(&self) {
        self.empty_reads.fetch_add(1, Ordering::Relaxed);
    }

    /// The lookups, hits, store reads and empty reads so far.
    pub fn get(&self) -> (u64, u64, u64, u64) {
        (
            self.lookups.load(Ordering::Relaxed),
            self.hits.load(Ordering::Relaxed),
            self.store_reads.load(Ordering::Relaxed),
            self.empty_reads.load(Ordering::Relaxed),
        )
    }
}
