/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/persistent_hnsw.go (searchPersistentLayer, Search, SearchWithOptions,
 * SearchWithUidAndOptions, SearchWithUid, SearchWithPath). Changed for ultragraphdb: translated to Rust; a
 * neighbour whose vector read fails ends the search with the error, where Dgraph skips it as it skips an absent
 * vector; a node without the level searched, or a layer result left empty by a breadth or result count of 0, is an
 * error where Dgraph panics; the search-time metric is not recorded, storage code reading no clock.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_traits_index::{
    CacheType, Metric, SearchFilter, SearchPathResult, VectorIndexOptions,
};

use crate::errors::hnsw_error::HnswError;
use crate::traits::hnsw_float::HnswFloat;
use crate::types::candidate_heap::CandidateHeap;
use crate::types::persistent_heap_element::PersistentHeapElement;
use crate::types::persistent_hnsw::PersistentHnsw;
use crate::types::search_layer_result::SearchLayerResult;
use crate::utils::helper::is_equal;

impl<T: HnswFloat> PersistentHnsw<T> {
    /// Dgraph's `searchPersistentLayer`: the nearest neighbours of `query` on `level`, starting from `entry`, whose
    /// vector is `start_vec`. The search keeps `expected_neighbors` neighbours the filter keeps, and stops once its
    /// best candidate cannot improve them.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn search_persistent_layer<C: CacheType<Error = HnswError>>(
        &self,
        c: &C,
        level: usize,
        entry: u64,
        start_vec: &[T],
        query: &[T],
        entry_is_filtered_out: bool,
        expected_neighbors: usize,
        filter: SearchFilter<T>,
    ) -> Result<SearchLayerResult<T>, HnswError> {
        let sim = self.sim_type;
        let expected = expected_neighbors as i64;
        let mut r = SearchLayerResult::new(level);
        let best_dist = sim.distance_score(start_vec, query)?;
        r.mark_first_distance_computation();
        let best = PersistentHeapElement::new(best_dist, entry, entry_is_filtered_out);
        r.set_first_path_node(best);
        let mut candidates = CandidateHeap::build(vec![best], sim.is_similarity_metric());

        while let Some(curr) = candidates.pop() {
            if r.num_neighbors() >= expected
                && sim.is_better_score(r.last_neighbor_score()?, curr.value())
            {
                // Once the best candidate cannot improve the neighbours kept, the layer is done.
                break;
            }
            let Some(all_layer_edges) = self.fill_neighbor_edges(curr.index(), c)? else {
                continue;
            };
            let row = all_layer_edges.get(level).ok_or_else(|| {
                HnswError::LevelMissing(curr.index(), level, all_layer_edges.len())
            })?;
            let mut improved = false;
            for &curr_uid in row {
                if r.index_visited(curr_uid) {
                    continue;
                }
                let e_vec = self.vec_or_empty(curr_uid, c)?;
                if e_vec.is_empty() {
                    continue;
                }
                let curr_dist = sim.distance_score(&e_vec, query)?;
                let filtered_out = !filter(query, &e_vec, curr_uid);
                let curr_element = PersistentHeapElement::new(curr_dist, curr_uid, filtered_out);
                r.add_to_visited(curr_element);
                r.increment_distance_computations();

                // Until `expected` neighbours are kept any candidate is; then only a better one. Filtered elements
                // do not count towards `expected`.
                if r.num_neighbors() < expected
                    || sim.is_better_score(curr_dist, r.last_neighbor_score()?)
                {
                    if candidates.len() > expected_neighbors {
                        candidates.pop_last();
                    }
                    candidates.push(curr_element);
                    r.add_path_node(curr_element, &sim, expected_neighbors)?;
                    improved = true;
                }
            }
            if !improved && r.num_neighbors() >= expected {
                break;
            }
        }
        Ok(r)
    }

    /// Dgraph's `Search`: the uids of at most `max_results` vectors nearest `query` that pass `filter`, best first.
    ///
    /// # Errors
    ///
    /// [`HnswError::EmptyTree`] for an index holding no vector, which [`HnswError::is_empty_index`] recognises as
    /// no neighbours; a failed read; a stored value that does not decode.
    pub fn search<C: CacheType<Error = HnswError>>(
        &self,
        c: &C,
        query: &[T],
        max_results: usize,
        filter: SearchFilter<T>,
    ) -> Result<Vec<u64>, HnswError> {
        self.search_with_path(c, query, max_results, filter)
            .map(|r| r.neighbors().to_vec())
    }

    /// Dgraph's `SearchWithOptions`: [`PersistentHnsw::search`] with an `efOverride` used on every layer and as the
    /// bottom layer's breadth when it exceeds `max_results`, and a distance threshold: a euclidean distance, or a
    /// cosine distance (`1 - similarity`); dot product ignores it.
    pub fn search_with_options<C: CacheType<Error = HnswError>>(
        &self,
        c: &C,
        query: &[T],
        max_results: usize,
        opts: &VectorIndexOptions<T>,
    ) -> Result<Vec<u64>, HnswError> {
        let filter = opts.filter();
        let (mut entry, mut start_vec) = self.pick_start_node(c)?;
        let ef_upper = if opts.ef_override() > 0 {
            opts.ef_override()
        } else {
            self.ef_search
        };
        for level in 0..self.max_levels - 1 {
            if is_equal(&start_vec, query) {
                break;
            }
            let filter_out = !filter(query, &start_vec, entry);
            let layer = self.search_persistent_layer(
                c, level, entry, &start_vec, query, filter_out, ef_upper, filter,
            )?;
            entry = layer.best_neighbor()?.index();
            start_vec = self.vec_from_uid(entry, c)?;
        }
        let filter_out = !filter(query, &start_vec, entry);
        let candidate_k = max_results.max(ef_upper);
        let layer = self.search_persistent_layer(
            c,
            self.max_levels - 1,
            entry,
            &start_vec,
            query,
            filter_out,
            candidate_k,
            filter,
        )?;
        Ok(self.final_neighbors(&layer, max_results, opts.distance_threshold()))
    }

    /// Dgraph's `SearchWithUidAndOptions`: [`PersistentHnsw::search_with_options`] from the vector of `query_uid`,
    /// on the bottom layer alone, its breadth the larger of `max_results` and the `efOverride`. A uid without a
    /// vector has no neighbours.
    pub fn search_with_uid_and_options<C: CacheType<Error = HnswError>>(
        &self,
        c: &C,
        query_uid: u64,
        max_results: usize,
        opts: &VectorIndexOptions<T>,
    ) -> Result<Vec<u64>, HnswError> {
        let filter = opts.filter();
        let query_vec = match self.vec_from_uid(query_uid, c) {
            Err(e) if e.is_fetching_posting_list() => return Ok(Vec::new()),
            Err(e) => return Err(e),
            Ok(v) => v,
        };
        if query_vec.is_empty() {
            return Ok(Vec::new());
        }
        let filter_out = !filter(&query_vec, &query_vec, query_uid);
        let candidate_k = max_results.max(opts.ef_override());
        let layer = self.search_persistent_layer(
            c,
            self.max_levels - 1,
            query_uid,
            &query_vec,
            &query_vec,
            filter_out,
            candidate_k,
            filter,
        )?;
        Ok(self.final_neighbors(&layer, max_results, opts.distance_threshold()))
    }

    /// Dgraph's `SearchWithUid`: the bottom layer's neighbours of the vector of `query_uid`, filtered ones included,
    /// as Dgraph returns them. A uid without a vector has no neighbours.
    pub fn search_with_uid<C: CacheType<Error = HnswError>>(
        &self,
        c: &C,
        query_uid: u64,
        max_results: usize,
        filter: SearchFilter<T>,
    ) -> Result<Vec<u64>, HnswError> {
        let query_vec = match self.vec_from_uid(query_uid, c) {
            Err(e) if e.is_fetching_posting_list() => return Ok(Vec::new()),
            Err(e) => return Err(e),
            Ok(v) => v,
        };
        if query_vec.is_empty() {
            return Ok(Vec::new());
        }
        let filter_out = !filter(&query_vec, &query_vec, query_uid);
        let layer = self.search_persistent_layer(
            c,
            self.max_levels - 1,
            query_uid,
            &query_vec,
            &query_vec,
            filter_out,
            max_results,
            filter,
        )?;
        Ok(layer.neighbors.iter().map(|n| n.index()).collect())
    }

    /// Dgraph's `SearchWithPath`: [`PersistentHnsw::search`] with the path from the entry point and the search's
    /// counters: distance computations and the vectors visited on each level.
    pub fn search_with_path<C: CacheType<Error = HnswError>>(
        &self,
        c: &C,
        query: &[T],
        max_results: usize,
        filter: SearchFilter<T>,
    ) -> Result<SearchPathResult, HnswError> {
        let mut r = SearchPathResult::new();
        let (mut entry, mut start_vec) = self.pick_start_node(c)?;
        for level in 0..self.max_levels - 1 {
            if is_equal(&start_vec, query) {
                break;
            }
            let filter_out = !filter(query, &start_vec, entry);
            let layer = self.search_persistent_layer(
                c,
                level,
                entry,
                &start_vec,
                query,
                filter_out,
                self.ef_search,
                filter,
            )?;
            layer.update_final_metrics(&mut r);
            entry = layer.best_neighbor()?.index();
            layer.update_final_path(&mut r);
            start_vec = self.vec_from_uid(entry, c)?;
        }
        let filter_out = !filter(query, &start_vec, entry);
        let layer = self.search_persistent_layer(
            c,
            self.max_levels - 1,
            entry,
            &start_vec,
            query,
            filter_out,
            max_results,
            filter,
        )?;
        layer.update_final_metrics(&mut r);
        layer.update_final_path(&mut r);
        layer.add_final_neighbors(&mut r);
        Ok(r)
    }

    /// The bottom layer's neighbours the filter and the threshold keep, at most `max_results`.
    fn final_neighbors(
        &self,
        layer: &SearchLayerResult<T>,
        max_results: usize,
        threshold: Option<f64>,
    ) -> Vec<u64> {
        let metric = self.sim_type.metric();
        layer
            .neighbors
            .iter()
            .filter(|n| !n.filtered_out())
            .filter(|n| match (threshold, metric) {
                (Some(th), Metric::Euclidean) => !exceeds(n.value().to_f64(), th),
                (Some(th), Metric::Cosine) => !exceeds(1.0 - n.value().to_f64(), th),
                _ => true,
            })
            .map(|n| n.index())
            .take(max_results)
            .collect()
    }
}

/// Whether `distance` lies beyond `threshold`, as Dgraph compares them: a NaN distance does not.
fn exceeds(distance: f64, threshold: f64) -> bool {
    distance > threshold
}
