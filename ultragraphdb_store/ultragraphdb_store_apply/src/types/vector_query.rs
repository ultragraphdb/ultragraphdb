/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_index::VectorIndexOptions;

/// A nearest-neighbour query of a vector index ([`crate::ChangesetApplier::search_vectors`]): the vector searched
/// for, the most nodes it finds, and the options the search runs with, as Dgraph's `similar_to` takes them.
#[derive(Debug, Clone, Copy)]
pub struct VectorQuery<'q> {
    pub(crate) vector: &'q [f32],
    pub(crate) k: usize,
    pub(crate) options: &'q VectorIndexOptions<f32>,
}

impl<'q> VectorQuery<'q> {
    /// The query for at most `k` nodes nearest `vector`, searched as `options` ask.
    pub fn new(vector: &'q [f32], k: usize, options: &'q VectorIndexOptions<f32>) -> Self {
        Self { vector, k, options }
    }
}
