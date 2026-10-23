/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/persistent_hnsw.go (persistentHNSW as index.VectorIndex,
 * index.OptionalIndexSupport and index.OptionalSearchOptions). Changed for ultragraphdb: translated to Rust; an
 * insert takes the transaction it writes through and builds the transaction's cache, where Go's takes a cache and
 * returns nothing for any other than a `*TxnCache`; the level draw is the caller's.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_traits_index::{
    CacheType, KeyValue, OptionalIndexSupport, OptionalSearchOptions, SearchFilter,
    SearchPathResult, Txn, VectorIndex, VectorIndexOptions,
};

use crate::errors::hnsw_error::HnswError;
use crate::traits::hnsw_float::HnswFloat;
use crate::types::persistent_hnsw::PersistentHnsw;
use crate::types::txn_cache::TxnCache;

impl<T: HnswFloat> OptionalIndexSupport<T> for PersistentHnsw<T> {
    type Error = HnswError;

    fn search_with_path<C: CacheType<Error = Self::Error>>(
        &self,
        c: &C,
        query: &[T],
        max_results: usize,
        filter: SearchFilter<T>,
    ) -> Result<SearchPathResult, Self::Error> {
        PersistentHnsw::search_with_path(self, c, query, max_results, filter)
    }
}

impl<T: HnswFloat> VectorIndex<T> for PersistentHnsw<T> {
    fn search<C: CacheType<Error = Self::Error>>(
        &self,
        c: &C,
        query: &[T],
        max_results: usize,
        filter: SearchFilter<T>,
    ) -> Result<Vec<u64>, Self::Error> {
        PersistentHnsw::search(self, c, query, max_results, filter)
    }

    fn search_with_uid<C: CacheType<Error = Self::Error>>(
        &self,
        c: &C,
        query_uid: u64,
        max_results: usize,
        filter: SearchFilter<T>,
    ) -> Result<Vec<u64>, Self::Error> {
        PersistentHnsw::search_with_uid(self, c, query_uid, max_results, filter)
    }

    fn insert<X: Txn<Error = Self::Error>>(
        &self,
        txn: &X,
        uuid: u64,
        vec: &[T],
        draw: &mut impl FnMut() -> f64,
    ) -> Result<Vec<KeyValue>, Self::Error> {
        let tc = TxnCache::new(txn, txn.start_ts());
        PersistentHnsw::insert(self, &tc, uuid, vec, draw)
    }
}

impl<T: HnswFloat> OptionalSearchOptions<T> for PersistentHnsw<T> {
    fn search_with_options<C: CacheType<Error = Self::Error>>(
        &self,
        c: &C,
        query: &[T],
        max_results: usize,
        opts: &VectorIndexOptions<T>,
    ) -> Result<Vec<u64>, Self::Error> {
        PersistentHnsw::search_with_options(self, c, query, max_results, opts)
    }

    fn search_with_uid_and_options<C: CacheType<Error = Self::Error>>(
        &self,
        c: &C,
        query_uid: u64,
        max_results: usize,
        opts: &VectorIndexOptions<T>,
    ) -> Result<Vec<u64>, Self::Error> {
        PersistentHnsw::search_with_uid_and_options(self, c, query_uid, max_results, opts)
    }
}
