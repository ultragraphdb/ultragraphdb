/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/index/index.go (SearchFilter, AcceptAll, AcceptNone, VectorIndexOptions).
 * Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::traits::float::Float;

/// Dgraph's `SearchFilter`: whether a search keeps a result, given the query, the result's vector and its uid.
pub type SearchFilter<T> = fn(query: &[T], result: &[T], uid: u64) -> bool;

/// Dgraph's `AcceptAll`.
pub fn accept_all<T: Float>(_: &[T], _: &[T], _: u64) -> bool {
    true
}

/// Dgraph's `AcceptNone`.
pub fn accept_none<T: Float>(_: &[T], _: &[T], _: u64) -> bool {
    false
}

/// Dgraph's `VectorIndexOptions`: per-call search tuning; unset fields override nothing.
#[derive(Debug, Clone, Copy)]
pub struct VectorIndexOptions<T: Float> {
    ef_override: usize,
    distance_threshold: Option<f64>,
    filter: SearchFilter<T>,
}

impl<T: Float> Default for VectorIndexOptions<T> {
    fn default() -> Self {
        Self {
            ef_override: 0,
            distance_threshold: None,
            filter: accept_all::<T>,
        }
    }
}

impl<T: Float> VectorIndexOptions<T> {
    /// Options with the search breadth `ef_override` (0 for none), the metric-domain `distance_threshold` and
    /// `filter`.
    pub fn new(
        ef_override: usize,
        distance_threshold: Option<f64>,
        filter: SearchFilter<T>,
    ) -> Self {
        Self {
            ef_override,
            distance_threshold,
            filter,
        }
    }

    /// The search breadth for this call, 0 for the index's own.
    pub fn ef_override(&self) -> usize {
        self.ef_override
    }

    /// The largest distance a result may have: euclidean distance, or cosine distance in [0, 2].
    pub fn distance_threshold(&self) -> Option<f64> {
        self.distance_threshold
    }

    /// The filter results pass.
    pub fn filter(&self) -> SearchFilter<T> {
        self.filter
    }
}
