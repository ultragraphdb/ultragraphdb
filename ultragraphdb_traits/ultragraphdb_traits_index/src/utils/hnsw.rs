/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/helper.go (the metric names). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// The euclidean metric's name.
pub const EUCLIDEAN: &str = "euclidean";
/// The cosine metric's name.
pub const COSINE: &str = "cosine";
/// The dot product metric's name.
pub const DOT_PROD: &str = "dotproduct";
