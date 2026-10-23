/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/persistent_hnsw.go (Insert, insertHelper) and helper.go (entryUuidInsert,
 * createEntryAndStartNodes, addStartNodeToAllLevels, newPersistentEdgeKeyValueEntry, uidScoreForNode,
 * addNeighbors, removeDeadNodes). Changed for ultragraphdb: translated to Rust; an insert takes the transaction's
 * cache by type, where Dgraph's Insert returns nothing for any other cache; the level draw comes from the caller's
 * source, where Dgraph reads its global random source; the neighbours whose inbound edges an insert adds are
 * written in uid order, where Dgraph iterates a map in random order; a read that fails is an error, where Dgraph
 * ignores the error of the entry point's and a neighbour list's read and takes a failed vector read for an absent
 * vector.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::{BTreeMap, HashMap, HashSet};

use ultragraphdb_core_keys::data_key;
use ultragraphdb_traits_index::{KeyValue, Txn, accept_all};

use crate::errors::hnsw_error::{HnswError, HnswErrorEnum};
use crate::traits::hnsw_float::HnswFloat;
use crate::types::persistent_hnsw::PersistentHnsw;
use crate::types::txn_cache::TxnCache;
use crate::utils::helper::{
    NOT_A_UID, dedupe_uids_preserve_order, insert_layer, insert_uids_top_k_by_score, parse_edges,
    uint64_to_bytes, worst_score,
};
use crate::utils::matrix::encode_uint64_matrix;

impl<T: HnswFloat> PersistentHnsw<T> {
    /// Dgraph's `Insert`: adds the vector `in_vec` of `in_uuid`, whose value the transaction already holds, to the
    /// graph, and returns the values it wrote: the node's neighbour lists, the inbound lists of its neighbours, and on
    /// the first insert the entry point. `draw` gives a uniform draw in [0, 1) once per insert that searches for
    /// neighbours, which picks the level the node is inserted from.
    ///
    /// Into an empty index, the node becomes the entry point with empty lists at every level. When the entry point
    /// has lost its vector, the insert makes the first node in key order holding one the entry point, with empty
    /// lists, and does not add `in_uuid`, as Dgraph does; when no node holds one, `in_uuid` becomes the entry point.
    ///
    /// # Errors
    ///
    /// A failed read or write, a stored value that does not decode, and a node without the levels searched.
    pub fn insert<X: Txn<Error = HnswError>>(
        &self,
        tc: &TxnCache<'_, X>,
        in_uuid: u64,
        in_vec: &[T],
        draw: &mut impl FnMut() -> f64,
    ) -> Result<Vec<KeyValue>, HnswError> {
        let (mut entry, mut edges) = self.create_entry_and_start_nodes(tc, in_uuid)?;
        if !edges.is_empty() {
            return Ok(edges);
        }
        if entry == in_uuid {
            // Inserting the entry point again writes nothing.
            return Ok(Vec::new());
        }
        let in_level = insert_layer(self.max_levels, draw());
        for level in 0..in_level {
            // Above the node's level, the search only finds a better start.
            let start_vec = self.vec_from_uid(entry, tc)?;
            let layer = self.search_persistent_layer(
                tc,
                level,
                entry,
                &start_vec,
                in_vec,
                false,
                self.ef_search,
                accept_all::<T>,
            )?;
            entry = layer.best_neighbor()?.index();
        }

        self.add_neighbors(tc, in_uuid, vec![Vec::new(); self.max_levels])?;
        let mut outbound = vec![Vec::new(); self.max_levels];
        let mut inbound: BTreeMap<u64, Vec<Vec<u64>>> = BTreeMap::new();
        for (level, out) in outbound.iter_mut().enumerate().skip(in_level) {
            let start_vec = self.vec_from_uid(entry, tc)?;
            let layer = self.search_persistent_layer(
                tc,
                level,
                entry,
                &start_vec,
                in_vec,
                false,
                self.ef_construction,
                accept_all::<T>,
            )?;
            entry = layer.best_neighbor()?.index();
            for nn in &layer.neighbors {
                inbound
                    .entry(nn.index())
                    .or_insert_with(|| vec![Vec::new(); self.max_levels])[level]
                    .push(in_uuid);
                out.push(nn.index());
            }
        }
        edges.push(self.add_neighbors(tc, in_uuid, outbound)?);
        for (nn_uid, inbound_edges) in inbound {
            edges.push(self.add_neighbors(tc, nn_uid, inbound_edges)?);
        }
        Ok(edges)
    }

    /// Dgraph's `createEntryAndStartNodes`: the entry point, and no values when it holds a vector; when the index is
    /// empty or the entry point lost its vector, the values making a node the entry point.
    fn create_entry_and_start_nodes<X: Txn<Error = HnswError>>(
        &self,
        tc: &TxnCache<'_, X>,
        in_uuid: u64,
    ) -> Result<(u64, Vec<KeyValue>), HnswError> {
        let txn = tc.txn();
        let entry_key = data_key(&self.vec_entry_key, 1)?;
        txn.lock_key(&entry_key);
        let result = self.entry_or_start_nodes(tc, &entry_key, in_uuid);
        txn.unlock_key(&entry_key);
        result
    }

    fn entry_or_start_nodes<X: Txn<Error = HnswError>>(
        &self,
        tc: &TxnCache<'_, X>,
        entry_key: &[u8],
        in_uuid: u64,
    ) -> Result<(u64, Vec<KeyValue>), HnswError> {
        let txn = tc.txn();
        let create_edges = |uuid: u64| {
            self.add_start_node_to_all_levels(entry_key, txn, uuid)
                .map(|edges| (0, edges))
        };
        let data = match txn.get_with_lock_held(entry_key) {
            Ok(data) if !data.is_empty() => data,
            Ok(_) => return create_edges(in_uuid),
            Err(e) if e.is_absence() => return create_edges(in_uuid),
            Err(e) => return Err(e),
        };
        let entry = self.entry_uid(&data)?;
        if !self.vec_or_empty(entry, tc)?.is_empty() {
            return Ok((entry, Vec::new()));
        }
        // The entry point's vector was deleted: the first node holding a vector becomes the entry point.
        match self.calculate_new_entry_vec(tc) {
            Ok((new_entry, _)) => create_edges(new_entry),
            Err(HnswError(HnswErrorEnum::EmptyTree(_))) => create_edges(in_uuid),
            Err(e) => Err(e),
        }
    }

    /// Dgraph's `addStartNodeToAllLevels`: empty neighbour lists at every level for `in_uuid`, and the entry point
    /// naming it.
    fn add_start_node_to_all_levels<X: Txn<Error = HnswError>>(
        &self,
        entry_key: &[u8],
        txn: &X,
        in_uuid: u64,
    ) -> Result<Vec<KeyValue>, HnswError> {
        let key = data_key(&self.vec_key, in_uuid)?;
        let empty_edges = encode_uint64_matrix(&vec![Vec::new(); self.max_levels]);
        let edge = KeyValue::new(in_uuid, self.vec_key.clone(), empty_edges);
        txn.lock_key(&key);
        let written = txn.add_mutation_with_lock_held(&key, &edge);
        txn.unlock_key(&key);
        written?;
        // Dgraph's entryUuidInsert.
        let entry = KeyValue::new(1, self.vec_entry_key.clone(), uint64_to_bytes(in_uuid));
        txn.add_mutation_with_lock_held(entry_key, &entry)?;
        Ok(vec![edge, entry])
    }

    /// Dgraph's `uidScoreForNode`: the score of `uid` against `node`, whose vector is `node_vec`; the metric's worst
    /// score for `notAUid`, `node` itself, a uid without a vector, and vectors of different lengths.
    fn uid_score_for_node<X: Txn<Error = HnswError>>(
        &self,
        tc: &TxnCache<'_, X>,
        node: u64,
        node_vec: &[T],
        uid: u64,
    ) -> Result<T, HnswError> {
        let worst = worst_score(self.sim_type);
        if uid == NOT_A_UID || uid == node {
            return Ok(worst);
        }
        let out_vec = self.vec_or_empty(uid, tc)?;
        if out_vec.is_empty() {
            return Ok(worst);
        }
        Ok(self
            .sim_type
            .distance_score(node_vec, &out_vec)
            .unwrap_or(worst))
    }

    /// Dgraph's `addNeighbors`: adds `all_layer_neighbors` to the neighbour lists of `uuid`, level by level, each
    /// list kept sorted best to worst and at most `efConstruction` long, dead nodes removed, and writes them.
    fn add_neighbors<X: Txn<Error = HnswError>>(
        &self,
        tc: &TxnCache<'_, X>,
        uuid: u64,
        all_layer_neighbors: Vec<Vec<u64>>,
    ) -> Result<KeyValue, HnswError> {
        let txn = tc.txn();
        let key = data_key(&self.vec_key, uuid)?;
        txn.lock_key(&key);
        let result = self.neighbours_added(tc, &key, uuid, all_layer_neighbors);
        txn.unlock_key(&key);
        result
    }

    fn neighbours_added<X: Txn<Error = HnswError>>(
        &self,
        tc: &TxnCache<'_, X>,
        key: &[u8],
        uuid: u64,
        all_layer_neighbors: Vec<Vec<u64>>,
    ) -> Result<KeyValue, HnswError> {
        let txn = tc.txn();
        let cached = self.node_all_edges.borrow().get(&uuid).cloned();
        let mut all_layer_edges = match cached {
            Some(edges) => edges,
            None => match txn.get_with_lock_held(key) {
                Ok(data) if !data.is_empty() => self.decode_edges(uuid, &data)?,
                Ok(_) => all_layer_neighbors.clone(),
                Err(e) if e.is_absence() => all_layer_neighbors.clone(),
                Err(e) => return Err(e),
            },
        };
        let levels = all_layer_edges.len();
        let mut in_vec: Option<Vec<T>> = None;
        for level in 0..self.max_levels {
            let row = all_layer_edges
                .get_mut(level)
                .ok_or_else(|| HnswError::LevelMissing(uuid, level, levels))?;
            *row = self.remove_dead_nodes(std::mem::take(row), tc)?;
            let new_level = all_layer_neighbors
                .get(level)
                .ok_or_else(|| HnswError::LevelMissing(uuid, level, all_layer_neighbors.len()))?;
            // Only the levels gaining neighbours need work: an inbound edge changes one.
            if new_level.is_empty() {
                continue;
            }
            if in_vec.is_none() {
                let v = self.vec_or_empty(uuid, tc)?;
                if v.is_empty() {
                    // Without the node's vector the edges cannot be scored: append, de-duplicate and truncate.
                    row.extend_from_slice(new_level);
                    *row = dedupe_uids_preserve_order(std::mem::take(row));
                    row.truncate(self.ef_construction);
                    continue;
                }
                in_vec = Some(v);
            }
            let node_vec = in_vec.as_deref().unwrap_or_default();
            let mut score_cache: HashMap<u64, T> = HashMap::new();
            let mut score = |uid: u64| -> Result<T, HnswError> {
                if let Some(s) = score_cache.get(&uid) {
                    return Ok(*s);
                }
                let s = self.uid_score_for_node(tc, uuid, node_vec, uid)?;
                score_cache.insert(uid, s);
                Ok(s)
            };
            let new_uids: Vec<u64> = if uuid != NOT_A_UID {
                new_level.iter().copied().filter(|&u| u != uuid).collect()
            } else {
                new_level.clone()
            };
            *row = insert_uids_top_k_by_score(
                std::mem::take(row),
                &new_uids,
                self.ef_construction,
                self.sim_type,
                &mut score,
            )?;
        }

        self.node_all_edges
            .borrow_mut()
            .insert(uuid, all_layer_edges.clone());
        let edge = KeyValue::new(
            uuid,
            self.vec_key.clone(),
            encode_uint64_matrix(&all_layer_edges),
        );
        txn.add_mutation_with_lock_held(key, &edge)?;
        Ok(edge)
    }

    /// Dgraph's `removeDeadNodes`: `nn_edges` without the dead list's uids, the list read once per index.
    fn remove_dead_nodes<X: Txn<Error = HnswError>>(
        &self,
        nn_edges: Vec<u64>,
        tc: &TxnCache<'_, X>,
    ) -> Result<Vec<u64>, HnswError> {
        if self.dead_nodes.borrow().is_none() {
            let dead: HashSet<u64> = match self.get_data_or_none(&self.vec_dead, 1, tc)? {
                Some(data) => parse_edges(&String::from_utf8_lossy(&data))?
                    .into_iter()
                    .collect(),
                None => HashSet::new(),
            };
            *self.dead_nodes.borrow_mut() = Some(dead);
        }
        let dead = self.dead_nodes.borrow();
        match dead.as_ref() {
            Some(dead) if !dead.is_empty() => Ok(nn_edges
                .into_iter()
                .filter(|uid| !dead.contains(uid))
                .collect()),
            _ => Ok(nn_edges),
        }
    }
}
