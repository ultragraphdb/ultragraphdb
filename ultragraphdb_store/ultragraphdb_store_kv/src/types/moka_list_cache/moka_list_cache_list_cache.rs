/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::ListCache;

use crate::types::moka_list_cache::MokaListCache;

impl<V: Clone + Send + Sync + 'static> ListCache for MokaListCache<V> {
    type Value = V;

    fn get(&self, key: &[u8]) -> Option<V> {
        self.cache.get(key)
    }

    fn insert(&self, key: Vec<u8>, value: V) {
        self.cache.insert(key, value);
    }

    fn remove(&self, key: &[u8]) {
        self.cache.invalidate(key);
    }
}
