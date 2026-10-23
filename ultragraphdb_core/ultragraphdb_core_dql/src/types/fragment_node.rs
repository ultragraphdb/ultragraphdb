/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::graph_query::GraphQuery;

/// A fragment definition, with its state in the depth-first expansion that detects cycles.
#[derive(Debug, Clone, Default)]
pub(crate) struct FragmentNode {
    pub name: String,
    pub gq: GraphQuery,
    /// Entered in the depth-first expansion.
    pub entered: bool,
    /// Exited in the depth-first expansion.
    pub exited: bool,
}
