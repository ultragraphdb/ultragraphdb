/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::hash::RandomState;
use std::sync::atomic::{AtomicU64, Ordering};

use moka::sync::Cache;

/// The values the keyspace without versions was found to hold, by key: the term dictionary's entries, which a write
/// reads at least once per term it names. A read that finds a value fills the cache; a write or clear of the keyspace
/// invalidates what it changed after it commits, and moves the generation on, so that a read racing it does not fill
/// the value it read before the change. Bounded by the bytes of its keys and values.
pub(crate) struct TermCache {
    cache: Cache<Vec<u8>, Vec<u8>, RandomState>,
    // An atomic (Ultra rule 9): readers on any thread compare it before and after a read; writers move it on under
    // the store's writer lock. SeqCst, deliberately: a reader fills the cache and then loads the generation, a writer
    // moves the generation on and then invalidates, and only one total order guarantees that the reader sees the new
    // generation or the writer's invalidation removes the filled value.
    generation: AtomicU64,
}

impl TermCache {
    /// A cache holding keys and values up to `max_bytes` in total.
    #[expect(
        clippy::disallowed_methods,
        reason = "the cache hashes with per-process random keys against collisions chosen by clients; the simulator's \
                  store holds no cache"
    )]
    pub fn new(max_bytes: u64) -> Self {
        let cache = Cache::builder()
            .max_capacity(max_bytes)
            .weigher(|key: &Vec<u8>, value: &Vec<u8>| {
                u32::try_from(key.len() + value.len()).unwrap_or(u32::MAX)
            })
            .build_with_hasher(RandomState::new());
        Self {
            cache,
            generation: AtomicU64::new(0),
        }
    }

    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.cache.get(key)
    }

    /// The generation a read takes before it reads the keyspace and passes to [`TermCache::fill`].
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// Caches the value a read at `generation` found, unless a change committed since.
    pub fn fill(&self, key: &[u8], value: Vec<u8>, generation: u64) {
        self.cache.insert(key.to_vec(), value);
        if self.generation() != generation {
            self.cache.invalidate(key);
        }
    }

    /// Invalidates `keys`, which a committed write changed.
    pub fn invalidate<'k>(&self, keys: impl IntoIterator<Item = &'k [u8]>) {
        let _previous: u64 = self.generation.fetch_add(1, Ordering::SeqCst);
        keys.into_iter().for_each(|k| self.cache.invalidate(k));
    }

    /// Invalidates every key, the keyspace having been cleared.
    pub fn clear(&self) {
        let _previous: u64 = self.generation.fetch_add(1, Ordering::SeqCst);
        self.cache.invalidate_all();
    }
}
