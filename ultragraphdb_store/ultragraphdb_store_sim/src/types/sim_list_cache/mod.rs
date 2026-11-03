/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod sim_list_cache_list_cache;

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::types::sim_rng::SimRng;
use crate::utils::lock::lock;

/// The simulator's [`ultragraphdb_traits_store::ListCache`]: an ordered map of at most `capacity` entries that,
/// when full, evicts an entry its stream chooses, so its evictions are a function of the seed.
#[derive(Debug)]
pub struct SimListCache<V> {
    // A lock (Ultra rule 9): `ListCache` takes `&self` from any thread; held for one lookup or insertion.
    shared: Mutex<Shared<V>>,
    capacity: usize,
}

#[derive(Debug)]
struct Shared<V> {
    entries: BTreeMap<Vec<u8>, V>,
    rng: SimRng,
}

impl<V> SimListCache<V> {
    /// A cache of at most `capacity` entries, evicting as `rng` chooses.
    pub fn new(capacity: usize, rng: SimRng) -> Self {
        Self {
            shared: Mutex::new(Shared {
                entries: BTreeMap::new(),
                rng,
            }),
            capacity,
        }
    }

    /// How many entries the cache holds.
    pub fn len(&self) -> usize {
        lock(&self.shared).entries.len()
    }

    /// Whether the cache holds no entry.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Evicts one entry the stream chooses, as a cache may at any time; false when the cache is empty.
    pub fn evict_one(&self) -> bool {
        let mut shared = lock(&self.shared);
        let len = shared.entries.len();
        let at = shared.rng.index(len);
        let Some(key) = shared.entries.keys().nth(at).cloned() else {
            return false;
        };
        let _evicted: Option<V> = shared.entries.remove(&key);
        true
    }
}
