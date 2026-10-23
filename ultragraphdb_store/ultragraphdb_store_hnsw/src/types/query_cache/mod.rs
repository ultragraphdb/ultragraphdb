/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/helper.go (QueryCache, NewQueryCache). Changed for ultragraphdb: translated
 * to Rust; the cache is borrowed.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod query_cache_cache_type;

use ultragraphdb_traits_index::LocalCache;

/// Dgraph's `QueryCache`: the cache a search reads through, over a query's local cache.
#[derive(Debug)]
pub struct QueryCache<'c, L> {
    cache: &'c L,
    read_ts: u64,
}

impl<'c, L: LocalCache> QueryCache<'c, L> {
    /// Dgraph's `NewQueryCache`.
    pub fn new(cache: &'c L, read_ts: u64) -> Self {
        Self { cache, read_ts }
    }
}
