/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/helper.go (SimilarityType, GetSimType, insortPersistentHeapAscending,
 * insortPersistentHeapDescending, isBetterScoreForDistance, isBetterScoreForSimilarity). Changed for ultragraphdb: translated to Rust; the metric
 * as stage 1's `Metric`, its functions chosen by it; the float width is the vector type's, so no width is invalid.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_traits_index::Metric;

use crate::errors::hnsw_error::HnswError;
use crate::traits::hnsw_float::HnswFloat;
use crate::types::persistent_heap_element::PersistentHeapElement;
use crate::utils::distance::{cosine_similarity, dot_product, euclidean_distance};
use crate::utils::go_sort;

/// Dgraph's `SimilarityType`: a metric with its distance function, the order its scores rank in and how a layer
/// result keeps them sorted. Euclidean distance ranks low scores first; cosine similarity and dot product rank high
/// scores first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SimilarityType {
    metric: Metric,
}

impl SimilarityType {
    /// The similarity type of `metric`.
    pub fn new(metric: Metric) -> Self {
        Self { metric }
    }

    /// Dgraph's `GetSimType`: the metric named `index_type`, euclidean for any other name.
    pub fn from_name(index_type: &str) -> Self {
        Self::new(Metric::from_name(index_type).unwrap_or(Metric::Euclidean))
    }

    pub fn metric(&self) -> Metric {
        self.metric
    }

    /// The metric's name, Dgraph's `indexType`.
    pub fn index_type(&self) -> &'static str {
        self.metric.name()
    }

    /// Whether higher scores are better: cosine similarity and dot product.
    pub fn is_similarity_metric(&self) -> bool {
        !matches!(self.metric, Metric::Euclidean)
    }

    /// The score of `v` against `w`: Dgraph's `distanceScore`.
    ///
    /// # Errors
    ///
    /// [`HnswError::DifferentLengths`] for vectors of different lengths.
    pub fn distance_score<T: HnswFloat>(&self, v: &[T], w: &[T]) -> Result<T, HnswError> {
        match self.metric {
            Metric::Euclidean => euclidean_distance(v, w),
            Metric::Cosine => cosine_similarity(v, w),
            Metric::DotProduct => dot_product(v, w),
        }
    }

    /// Whether score `a` is better than `b`: Dgraph's `isBetterScore`.
    pub fn is_better_score<T: HnswFloat>(&self, a: T, b: T) -> bool {
        if self.is_similarity_metric() {
            a > b
        } else {
            a < b
        }
    }

    /// Inserts `val` into `slice`, sorted best first, after the elements of the same score: Dgraph's `insortHeap`.
    pub fn insort_heap<T: HnswFloat>(
        &self,
        slice: &mut Vec<PersistentHeapElement<T>>,
        val: PersistentHeapElement<T>,
    ) {
        let similarity = self.is_similarity_metric();
        let found = go_sort::search(slice.len(), |i| {
            Ok::<_, std::convert::Infallible>(if similarity {
                slice[i].value() < val.value()
            } else {
                slice[i].value() > val.value()
            })
        });
        let i = match found {
            Ok(i) => i,
            Err(never) => match never {},
        };
        slice.insert(i, val);
    }
}
