/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/helper.go (applyDistanceFunction, dotProduct, cosineSimilarity,
 * euclideanDistance). Changed for ultragraphdb: translated to Rust; the float width is the vector type's, so no
 * width is invalid.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::errors::hnsw_error::HnswError;
use crate::traits::hnsw_float::HnswFloat;

/// Dgraph's `applyDistanceFunction`: `kernel` of `a` and `b`.
fn apply_distance_function<T: HnswFloat>(
    a: &[T],
    b: &[T],
    func_name: &'static str,
    kernel: fn(&[T], &[T]) -> T,
) -> Result<T, HnswError> {
    if a.len() != b.len() {
        return Err(HnswError::DifferentLengths(func_name));
    }
    Ok(kernel(a, b))
}

/// Dgraph's `dotProduct`.
///
/// # Errors
///
/// [`HnswError::DifferentLengths`] for vectors of different lengths.
pub fn dot_product<T: HnswFloat>(a: &[T], b: &[T]) -> Result<T, HnswError> {
    apply_distance_function(a, b, "dot product", T::dot)
}

/// Dgraph's `cosineSimilarity`.
///
/// # Errors
///
/// [`HnswError::DifferentLengths`] for vectors of different lengths.
pub fn cosine_similarity<T: HnswFloat>(a: &[T], b: &[T]) -> Result<T, HnswError> {
    apply_distance_function(a, b, "cosine distance", T::cosine_sim)
}

/// Dgraph's `euclideanDistance`: the metric-domain distance, square-rooted, which a distance threshold compares
/// with directly.
///
/// # Errors
///
/// [`HnswError::DifferentLengths`] for vectors of different lengths.
pub fn euclidean_distance<T: HnswFloat>(a: &[T], b: &[T]) -> Result<T, HnswError> {
    apply_distance_function(a, b, "euclidean distance", T::euclidean)
}
