/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/index/index.go (VectorIndex, OptionalIndexSupport, OptionalSearchOptions).
 * Changed for ultragraphdb: translated to Rust; Go's `error` as an associated type, and Go's `context.Context`
 * left to the implementation.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::traits::cache_type::CacheType;
use crate::traits::float::Float;
use crate::types::key_value::KeyValue;
use crate::types::search_path_result::SearchPathResult;
use crate::types::vector_index_options::{SearchFilter, VectorIndexOptions};

/// Dgraph's `OptionalIndexSupport`: searches a vector index may support, or refuse as unsupported.
pub trait OptionalIndexSupport<T: Float> {
    /// Why a search failed.
    type Error;

    /// As [`VectorIndex::search`], with the search path and its metrics.
    fn search_with_path<C: CacheType>(
        &self,
        c: &C,
        query: &[T],
        max_results: usize,
        filter: SearchFilter<T>,
    ) -> Result<SearchPathResult, Self::Error>;
}

/// Dgraph's `VectorIndex`: searches for and adds vectors.
pub trait VectorIndex<T: Float>: OptionalIndexSupport<T> {
    /// The uids of at most `max_results` vectors nearest `query` that pass `filter`; the limit counts the results
    /// after filtering.
    fn search<C: CacheType>(
        &self,
        c: &C,
        query: &[T],
        max_results: usize,
        filter: SearchFilter<T>,
    ) -> Result<Vec<u64>, Self::Error>;

    /// As [`search`](Self::search), from the vector of `query_uid`.
    fn search_with_uid<C: CacheType>(
        &self,
        c: &C,
        query_uid: u64,
        max_results: usize,
        filter: SearchFilter<T>,
    ) -> Result<Vec<u64>, Self::Error>;

    /// Adds the vector `vec` of `uuid`; an existing `uuid` is an error.
    fn insert<C: CacheType>(
        &self,
        c: &C,
        uuid: u64,
        vec: &[T],
    ) -> Result<Vec<KeyValue>, Self::Error>;
}

/// Dgraph's `OptionalSearchOptions`: searches with per-call tuning.
pub trait OptionalSearchOptions<T: Float>: VectorIndex<T> {
    /// As [`VectorIndex::search`], tuned by `opts`.
    fn search_with_options<C: CacheType>(
        &self,
        c: &C,
        query: &[T],
        max_results: usize,
        opts: &VectorIndexOptions<T>,
    ) -> Result<Vec<u64>, Self::Error>;

    /// As [`VectorIndex::search_with_uid`], tuned by `opts`.
    fn search_with_uid_and_options<C: CacheType>(
        &self,
        c: &C,
        query_uid: u64,
        max_results: usize,
        opts: &VectorIndexOptions<T>,
    ) -> Result<Vec<u64>, Self::Error>;
}
