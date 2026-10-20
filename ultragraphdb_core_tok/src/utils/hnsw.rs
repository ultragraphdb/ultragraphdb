/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/persistent_factory.go and tok/hnsw/helper.go (their constants). Changed
 * for ultragraphdb: translated to Rust; the index itself is ported in stage 2.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// The hnsw option setting the default levels and breadths: maxLevels to it, efConstruction to 50 times it,
/// efSearch to 30 times it.
pub const EXPONENT_OPT: &str = "exponent";
/// The hnsw option for the number of graph levels.
pub const MAX_LEVELS_OPT: &str = "maxLevels";
/// The hnsw option for the search breadth while building.
pub const EF_CONSTRUCTION_OPT: &str = "efConstruction";
/// The hnsw option for the search breadth of a query.
pub const EF_SEARCH_OPT: &str = "efSearch";
/// The hnsw option for the similarity metric.
pub const METRIC_OPT: &str = "metric";
/// The hnsw factory's name.
pub const HNSW: &str = "hnsw";

/// The euclidean metric's name.
pub const EUCLIDEAN: &str = "euclidean";
/// The cosine metric's name.
pub const COSINE: &str = "cosine";
/// The dot product metric's name.
pub const DOT_PROD: &str = "dotproduct";
/// The error of a search in an empty index.
pub const EMPTY_HNSW_TREE_ERROR: &str = "HNSW tree has no elements";
/// The suffix of the predicate holding an index's vectors.
pub const VEC_KEYWORD: &str = "__vector_";
/// The suffix of the predicate holding an index's entry point.
pub const VEC_ENTRY: &str = "__vector_entry";
/// The suffix of the predicate holding an index's deleted nodes.
pub const VEC_DEAD: &str = "__vector_dead";
