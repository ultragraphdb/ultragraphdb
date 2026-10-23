/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::ListCache;

use crate::types::sim_list_cache::SimListCache;
use crate::utils::lock::lock;

impl<V: Clone> ListCache for SimListCache<V> {
    type Value = V;

    fn get(&self, key: &[u8]) -> Option<V> {
        lock(&self.shared).entries.get(key).cloned()
    }

    fn insert(&self, key: Vec<u8>, value: V) {
        if self.capacity == 0 {
            return;
        }
        let full = {
            let shared = lock(&self.shared);
            !shared.entries.contains_key(&key) && shared.entries.len() >= self.capacity
        };
        if full {
            self.evict_one();
        }
        lock(&self.shared).entries.insert(key, value);
    }

    fn remove(&self, key: &[u8]) {
        lock(&self.shared).entries.remove(key);
    }
}
