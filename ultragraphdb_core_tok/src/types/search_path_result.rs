/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/index/search_path.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::HashMap;

/// Dgraph's `SearchPathResult`: the result of a search with its path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchPathResult {
    neighbors: Vec<u64>,
    path: Vec<u64>,
    metrics: HashMap<String, u64>,
}

impl SearchPathResult {
    /// Dgraph's `NewSearchPathResult`: empty.
    pub fn new() -> Self {
        Self::default()
    }

    /// The nearest neighbors, in order, after filtering.
    pub fn neighbors(&self) -> &[u64] {
        &self.neighbors
    }

    /// The nearest neighbors, to fill.
    pub fn neighbors_mut(&mut self) -> &mut Vec<u64> {
        &mut self.neighbors
    }

    /// The path from the search's start to the nearest neighbor.
    pub fn path(&self) -> &[u64] {
        &self.path
    }

    /// The path, to fill.
    pub fn path_mut(&mut self) -> &mut Vec<u64> {
        &mut self.path
    }

    /// The named counters of the search.
    pub fn metrics(&self) -> &HashMap<String, u64> {
        &self.metrics
    }

    /// The counters, to fill.
    pub fn metrics_mut(&mut self) -> &mut HashMap<String, u64> {
        &mut self.metrics
    }
}
