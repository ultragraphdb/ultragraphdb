/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/helper.go (the metric names of SimilarityType). Changed for ultragraphdb:
 * translated to Rust; the metric's name, which the distance functions of stage 2 attach to.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::utils::hnsw::{COSINE, DOT_PROD, EUCLIDEAN};

/// The similarity metric of a vector index, the value of its `metric` option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Metric {
    /// `euclidean`, Dgraph's default.
    Euclidean,
    /// `cosine`.
    Cosine,
    /// `dotproduct`.
    DotProduct,
}

impl Metric {
    /// The metric named, as Dgraph's `metric` option accepts it.
    pub fn from_name(name: &str) -> Option<Metric> {
        match name {
            EUCLIDEAN => Some(Metric::Euclidean),
            COSINE => Some(Metric::Cosine),
            DOT_PROD => Some(Metric::DotProduct),
            _ => None,
        }
    }

    /// The metric's name.
    pub fn name(self) -> &'static str {
        match self {
            Metric::Euclidean => EUCLIDEAN,
            Metric::Cosine => COSINE,
            Metric::DotProduct => DOT_PROD,
        }
    }
}
