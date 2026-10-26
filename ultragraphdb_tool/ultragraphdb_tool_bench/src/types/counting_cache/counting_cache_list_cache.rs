/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::ListCache;

use crate::types::counting_cache::CountingCache;

impl<C: ListCache> ListCache for CountingCache<C> {
    type Value = C::Value;

    fn get(&self, key: &[u8]) -> Option<Self::Value> {
        let value = self.inner.get(key);
        self.counts.lookup(value.is_some());
        value
    }

    fn insert(&self, key: Vec<u8>, value: Self::Value) {
        self.inner.insert(key, value);
    }

    fn remove(&self, key: &[u8]) {
        self.inner.remove(key);
    }
}
