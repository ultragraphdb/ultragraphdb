/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU16;

/// A vector index a predicate declares: its factory, `hnsw`, the dimension of the vectors it indexes, and the options
/// it is built with, by name. A declaration states its dimension, 1 to 65,535 as a vector literal's, and one without
/// is refused when it is declared.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VectorIndexSpec {
    factory: String,
    dimension: Option<NonZeroU16>,
    options: Vec<(String, String)>,
}

impl VectorIndexSpec {
    /// The index of `factory` with `options`, stating no dimension.
    pub fn new(factory: impl Into<String>, options: Vec<(String, String)>) -> Self {
        Self {
            factory: factory.into(),
            dimension: None,
            options,
        }
    }

    /// The index indexing vectors of `dimension` elements.
    pub fn with_dimension(self, dimension: NonZeroU16) -> Self {
        Self {
            dimension: Some(dimension),
            ..self
        }
    }

    pub fn factory(&self) -> &str {
        &self.factory
    }

    /// The dimension the declaration states, `None` when it states none.
    pub fn dimension(&self) -> Option<NonZeroU16> {
        self.dimension
    }

    pub fn options(&self) -> &[(String, String)] {
        &self.options
    }
}
