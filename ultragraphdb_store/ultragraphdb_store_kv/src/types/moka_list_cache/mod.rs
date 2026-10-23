/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod moka_list_cache_debug;
mod moka_list_cache_list_cache;

use std::hash::RandomState;

use moka::sync::Cache;

/// The production [`ultragraphdb_traits_store::ListCache`]: moka's synchronous cache, built with `build_with_hasher`
/// and std's `RandomState`, keyed afresh in every process so that keys a client chooses, such as index terms, cannot
/// be made to collide; bounded by the total weight its weigher gives the entries. Clones share one cache. The
/// simulator replaces it with a deterministic map.
#[derive(Clone)]
pub struct MokaListCache<V> {
    cache: Cache<Vec<u8>, V, RandomState>,
}

impl<V: Clone + Send + Sync + 'static> MokaListCache<V> {
    /// A cache holding entries up to a total weight of `max_weight`, each weighed by `weigher` from its key and
    /// value.
    #[allow(
        clippy::disallowed_methods,
        reason = "the production cache hashes with per-process random keys against collisions chosen by clients; the \
                  simulator's deterministic map replaces it so that a run replays"
    )]
    pub fn new<W>(max_weight: u64, weigher: W) -> Self
    where
        W: Fn(&[u8], &V) -> u32 + Send + Sync + 'static,
    {
        let cache = Cache::builder()
            .max_capacity(max_weight)
            .weigher(move |key: &Vec<u8>, value: &V| weigher(key, value))
            .build_with_hasher(RandomState::new());
        Self { cache }
    }

    /// Runs moka's pending maintenance, evictions included, so that the cache's weight is up to date.
    pub fn run_pending_tasks(&self) {
        self.cache.run_pending_tasks();
    }

    /// The total weight of the entries, as of the last maintenance.
    pub fn weighted_size(&self) -> u64 {
        self.cache.weighted_size()
    }
}
