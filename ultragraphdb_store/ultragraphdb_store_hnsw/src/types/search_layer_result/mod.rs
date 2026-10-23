/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/search_layer.go. Changed for ultragraphdb: translated to Rust; the visited
 * set keeps uids, the only thing Dgraph reads of it; where Dgraph indexes an empty neighbour list and panics, the
 * port returns `NoNeighbour`.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::HashSet;

use ultragraphdb_traits_index::SearchPathResult;

use crate::errors::hnsw_error::HnswError;
use crate::traits::hnsw_float::HnswFloat;
use crate::types::persistent_heap_element::PersistentHeapElement;
use crate::types::similarity_type::SimilarityType;

/// The metric counting distance computations.
pub(crate) const DISTANCE_COMPUTATIONS: &str = "vector_distance_computations";
/// The prefix of the metric counting a level's visited vectors.
pub(crate) const VISITED_VECTORS_LEVEL: &str = "visited_vectors_level_";

/// Dgraph's `searchLayerResult`: the best candidates of one layer's search so far, best first, the uids it visited,
/// its path and its counters. Elements the filter leaves out may enter `neighbors`, which then grows by their number,
/// `filtered`.
#[derive(Debug, Clone)]
pub(crate) struct SearchLayerResult<T> {
    pub neighbors: Vec<PersistentHeapElement<T>>,
    visited: HashSet<u64>,
    path: Vec<u64>,
    metrics: Vec<(String, u64)>,
    level: usize,
    filtered: usize,
}

impl<T: HnswFloat> SearchLayerResult<T> {
    pub fn new(level: usize) -> Self {
        Self {
            neighbors: Vec::new(),
            visited: HashSet::new(),
            path: Vec::new(),
            metrics: Vec::new(),
            level,
            filtered: 0,
        }
    }

    pub fn set_first_path_node(&mut self, n: PersistentHeapElement<T>) {
        self.neighbors = vec![n];
        self.visited = HashSet::from([n.index()]);
        self.path = vec![n.index()];
    }

    /// Dgraph's `addPathNode`.
    ///
    /// # Errors
    ///
    /// [`HnswError::NoNeighbour`] when `max_results` and the filtered count are 0 and nothing is kept, where Dgraph
    /// panics.
    pub fn add_path_node(
        &mut self,
        n: PersistentHeapElement<T>,
        sim: &SimilarityType,
        max_results: usize,
    ) -> Result<(), HnswError> {
        sim.insort_heap(&mut self.neighbors, n);
        if n.filtered_out() {
            self.filtered += 1;
        }
        self.neighbors.truncate(max_results + self.filtered);
        let first = self.neighbors.first().ok_or_else(HnswError::NoNeighbour)?;
        if first.index() == n.index() {
            self.path.push(first.index());
        }
        Ok(())
    }

    /// The neighbours the filter keeps, which may be negative once filtered elements were truncated away.
    pub fn num_neighbors(&self) -> i64 {
        self.neighbors.len() as i64 - self.filtered as i64
    }

    pub fn mark_first_distance_computation(&mut self) {
        self.set_metric(DISTANCE_COMPUTATIONS, 1);
    }

    pub fn increment_distance_computations(&mut self) {
        let count = self.metric(DISTANCE_COMPUTATIONS) + 1;
        self.set_metric(DISTANCE_COMPUTATIONS, count);
    }

    /// The score of the last neighbour kept: Dgraph's `lastNeighborScore`.
    pub fn last_neighbor_score(&self) -> Result<T, HnswError> {
        self.neighbors
            .last()
            .map(|n| n.value())
            .ok_or_else(HnswError::NoNeighbour)
    }

    /// The best neighbour: Dgraph's `bestNeighbor`.
    pub fn best_neighbor(&self) -> Result<PersistentHeapElement<T>, HnswError> {
        self.neighbors
            .first()
            .copied()
            .ok_or_else(HnswError::NoNeighbour)
    }

    pub fn index_visited(&self, n: u64) -> bool {
        self.visited.contains(&n)
    }

    pub fn add_to_visited(&mut self, n: PersistentHeapElement<T>) {
        self.visited.insert(n.index());
    }

    /// Adds the layer's visited count and counters to `r`'s: Dgraph's `updateFinalMetrics`.
    pub fn update_final_metrics(&self, r: &mut SearchPathResult) {
        let visit_name = format!("{VISITED_VECTORS_LEVEL}{}", self.level);
        *r.metrics_mut().entry(visit_name).or_default() += self.visited.len() as u64;
        for (k, v) in &self.metrics {
            *r.metrics_mut().entry(k.clone()).or_default() += v;
        }
    }

    pub fn update_final_path(&self, r: &mut SearchPathResult) {
        r.path_mut().extend_from_slice(&self.path);
    }

    /// Dgraph's `addFinalNeighbors`: the neighbours the filter keeps.
    pub fn add_final_neighbors(&self, r: &mut SearchPathResult) {
        r.neighbors_mut().extend(
            self.neighbors
                .iter()
                .filter(|n| !n.filtered_out())
                .map(|n| n.index()),
        );
    }

    fn metric(&self, name: &str) -> u64 {
        self.metrics
            .iter()
            .find(|(k, _)| k == name)
            .map_or(0, |(_, v)| *v)
    }

    fn set_metric(&mut self, name: &str, value: u64) {
        match self.metrics.iter_mut().find(|(k, _)| k == name) {
            Some((_, v)) => *v = value,
            None => self.metrics.push((name.to_string(), value)),
        }
    }
}
