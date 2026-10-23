/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The posting cache's map, keyed by a list's key: moka in `ultragraphdb_store_kv`, a map with seeded eviction in
/// the simulator. The cache may evict any entry at any time; when an entry may be served, and when a fill is
/// accepted, is the caller's rule, carried in the values it stores.
pub trait ListCache {
    /// What the cache holds per key, cloned out on every hit: an `Arc` of the list and what its rule needs.
    type Value: Clone;

    /// The entry of `key`, if the cache holds one.
    fn get(&self, key: &[u8]) -> Option<Self::Value>;

    /// Stores `value` as the entry of `key`, replacing any entry it had.
    fn insert(&self, key: Vec<u8>, value: Self::Value);

    /// Removes the entry of `key`, if any.
    fn remove(&self, key: &[u8]);
}
