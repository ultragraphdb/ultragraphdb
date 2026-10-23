/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/helper.go (QueryCache's Get, Ts and Find). Changed for ultragraphdb: translated
 * to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_traits_index::{CacheType, LocalCache};

use crate::types::query_cache::QueryCache;

impl<L: LocalCache> CacheType for QueryCache<'_, L> {
    type Error = L::Error;

    fn get(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error> {
        self.cache.get(key)
    }

    fn ts(&self) -> u64 {
        self.read_ts
    }

    fn find(&self, prefix: &[u8], filter: impl Fn(&[u8]) -> bool) -> Result<u64, Self::Error> {
        self.cache.find(prefix, filter)
    }
}
