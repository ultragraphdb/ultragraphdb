/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/sharded_map.go. Changed for ultragraphdb: translated to Rust; Merge runs the
 * shards one after another instead of one goroutine each, which gives the same map.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::HashMap;

use crate::types::val::Val;

/// Dgraph's `NumShards`.
pub const NUM_SHARDS: usize = 30;

/// Dgraph's `ShardedMap`: values by uid in [`NUM_SHARDS`] maps, a uid's shard being `uid % NUM_SHARDS`.
#[derive(Debug, Clone, PartialEq)]
pub struct ShardedMap {
    shards: Vec<HashMap<u64, Val>>,
}

impl Default for ShardedMap {
    fn default() -> Self {
        Self::new()
    }
}

impl ShardedMap {
    /// Dgraph's `NewShardedMap`.
    pub fn new() -> Self {
        Self {
            shards: (0..NUM_SHARDS).map(|_| HashMap::new()).collect(),
        }
    }

    /// Dgraph's `Merge`: adds `other`'s values, combining a uid present in both with `ag(mine, theirs)`.
    pub fn merge<F: Fn(&Val, &Val) -> Val>(&mut self, other: &ShardedMap, ag: F) {
        for (mine, theirs) in self.shards.iter_mut().zip(&other.shards) {
            for (k, v) in theirs {
                let merged = match mine.get(k) {
                    Some(m) => ag(m, v),
                    None => v.clone(),
                };
                mine.insert(*k, merged);
            }
        }
    }

    /// Dgraph's `IsEmpty`: whether the map has no shards (not whether it holds no values).
    pub fn is_empty(&self) -> bool {
        self.shards.is_empty()
    }

    /// Dgraph's `GetShardOrNil`: shard `key`.
    pub fn shard(&self, key: usize) -> &HashMap<u64, Val> {
        &self.shards[key]
    }

    /// Dgraph's `Set`.
    pub fn set(&mut self, key: u64, value: Val) {
        self.shards[(key % NUM_SHARDS as u64) as usize].insert(key, value);
    }

    /// Dgraph's `Get`.
    pub fn get(&self, key: u64) -> Option<&Val> {
        self.shards[(key % NUM_SHARDS as u64) as usize].get(&key)
    }

    /// Dgraph's `Len`: the number of values.
    pub fn len(&self) -> usize {
        self.shards.iter().map(HashMap::len).sum()
    }

    /// Dgraph's `Iterate`: calls `f` on every value, shard by shard, stopping at the first error.
    pub fn iterate<E, F: FnMut(u64, &Val) -> Result<(), E>>(&self, mut f: F) -> Result<(), E> {
        self.shards
            .iter()
            .flat_map(|s| s.iter())
            .try_for_each(|(k, v)| f(*k, v))
    }
}
