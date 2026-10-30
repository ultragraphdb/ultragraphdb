/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The vector indexer and the vector search. A transaction writes a vector in its forward list alone; each round
//! applied queues the vectors it wrote or removed per predicate, in commit order, and a pass of the indexer inserts
//! them into the HNSW lists, or marks their nodes dead, ported from Dgraph's `posting/index.go` (`addIndexMutations`
//! for a vector, its dead list). A node's level is drawn from a stream of its own ([`LevelDraw::of_node`]). An entry
//! point whose vector is gone hands over, before a pass inserts, to the first node in key order the index holds with a
//! vector: the HNSW insert's own handover takes the forward lists' first vector, which may be one not yet indexed. A search
//! reads the HNSW lists and the vectors at its snapshot, so a vector committed and not yet indexed is not found: the
//! index's search reaches only the nodes the lists name, skipping one whose vector is not visible, and the one node it
//! may start from outside them, a vector the forward lists hold, is dropped unless the lists hold it.

use std::collections::BTreeSet;

use oxrdf::{GraphNameRef, NamedNodeRef};
use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey};
use ultragraphdb_store_hnsw::{
    QueryCache, VectorKeys, ViTxn, add_dead_node, bytes_as_float_array, bytes_to_uint64,
    index_posting, parse_edges, uint64_to_bytes,
};
use ultragraphdb_store_posting::{CachedList, DEL, Request};
use ultragraphdb_traits_index::{Txn, VectorIndex, VectorIndexOptions};
use ultragraphdb_traits_store::{ListCache, Scan, Store, StoreSnapshot};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_core::ChangesetCore;
use crate::types::level_draw::LevelDraw;
use crate::utils::lock::lock;
use crate::utils::vector_index::hnsw;

impl<S, C> ChangesetCore<S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// One pass of the vector indexer of `predicate` in `namespace`, as [`crate::ChangesetApplier::index_vectors`]
    /// runs it: under the apply lock, so no round is applied between its read at the watermark and its commit at the
    /// next version. A predicate whose vector index is being built is left to its build.
    pub fn index_vectors(
        &self,
        namespace: u64,
        predicate: NamedNodeRef<'_>,
    ) -> Result<u64, ChangesetError> {
        let _apply = lock(&self.apply);
        let refused = || ChangesetError::NoVectorIndex(predicate.as_str());
        let uid = self
            .quads
            .uid_of(namespace, predicate.as_str())?
            .ok_or_else(refused)?;
        let building = lock(&self.rebuild)
            .as_ref()
            .is_some_and(|r| r.namespace == namespace && r.predicate == uid && r.plan.build_vector);
        if building {
            return Ok(self.recorded(namespace, uid));
        }
        let catalog = self.catalog(namespace)?;
        let Some(spec) = catalog.predicate(uid).and_then(|d| d.vector_index()) else {
            // The queue of an index no longer declared goes with it.
            lock(&self.vectors).remove(&(namespace, uid));
            return Err(refused());
        };
        let lists = self.quads.lists();
        let watermark = lists.applied_ts();
        let pending = {
            let mut queues = lock(&self.vectors);
            let queue = queues.entry((namespace, uid)).or_default();
            std::mem::take(&mut queue.pending)
        };
        if !pending.is_empty() {
            let mut request = lists.request(watermark);
            let mut seen = BTreeSet::new();
            let mut graphs = BTreeSet::new();
            for (_, graph, node) in pending {
                if !seen.insert((graph, node)) {
                    continue;
                }
                let keys = VectorKeys::new(predicate.as_str(), namespace, graph, uid);
                if graphs.insert(graph) {
                    self.hand_over_entry(&mut request, &keys)?;
                }
                match vector_of(&mut request, &keys, node)? {
                    Some(vector) => {
                        unmark_dead(&mut request, &keys, node)?;
                        let index = hnsw(spec, keys)?;
                        let txn = ViTxn::new(&mut request);
                        let mut levels = LevelDraw::of_node(self.seed, namespace, uid, node);
                        VectorIndex::insert(&index, &txn, node, &vector, &mut || {
                            levels.next_f64()
                        })?;
                    }
                    None => mark_dead(&mut request, &keys, node)?,
                }
            }
            let mut batch = lists.batch();
            batch.push(request);
            lists.commit(batch)?;
        }
        lock(&self.vectors)
            .entry((namespace, uid))
            .or_default()
            .indexed = watermark;
        Ok(watermark)
    }

    /// The version the vector indexer of `predicate` in `namespace` has indexed through, as
    /// [`crate::ChangesetApplier::indexed_version`] reads it.
    pub fn indexed_version(
        &self,
        namespace: u64,
        predicate: NamedNodeRef<'_>,
    ) -> Result<u64, ChangesetError> {
        Ok(self
            .quads
            .uid_of(namespace, predicate.as_str())?
            .map_or(0, |uid| self.recorded(namespace, uid)))
    }

    /// Names the first node in key order the index at `keys` holds whose vector `request` sees the entry point, when the
    /// entry point's vector is gone; with none, the entry point is emptied, so the next insert starts the index afresh.
    fn hand_over_entry(
        &self,
        request: &mut Request<'_, S, C>,
        keys: &VectorKeys,
    ) -> Result<(), ChangesetError> {
        let entry = index_value(request, &keys.entry())?.and_then(|v| bytes_to_uint64(&v));
        let Some(entry) = entry else {
            return Ok(());
        };
        if vector_of(request, keys, entry)?.is_some() {
            return Ok(());
        }
        let lists = self.quads.lists();
        let read_ts = request.read_ts();
        lists.gate(read_ts)?;
        let snapshot = lists.store().snapshot(read_ts)?;
        // Every node's neighbour lists: the key of node 0's without its last 8 bytes, the node.
        let first = keys.neighbours(0);
        let prefix = first[..first.len() - 8].to_vec();
        let mut heir = Vec::new();
        for item in snapshot.scan(&Scan::prefix(prefix)) {
            let item = item?;
            let Ok((QuadKey::VectorIndex { node, .. }, None)) = QuadKey::parse(item.key()) else {
                continue;
            };
            if vector_of(request, keys, node)?.is_some() {
                heir = uint64_to_bytes(node);
                break;
            }
        }
        request
            .get(&keys.entry())?
            .add_posting(index_posting(heir), false);
        Ok(())
    }

    /// The version the indexer of predicate `uid` recorded.
    fn recorded(&self, namespace: u64, uid: u64) -> u64 {
        lock(&self.vectors)
            .get(&(namespace, uid))
            .map_or(0, |q| q.indexed)
    }

    /// The search of `predicate`'s vector index in `graph`, as [`crate::ChangesetApplier::search_vectors`] runs it.
    #[allow(clippy::too_many_arguments)]
    pub fn search_vectors(
        &self,
        namespace: u64,
        graph: GraphNameRef<'_>,
        predicate: NamedNodeRef<'_>,
        query: &[f32],
        k: usize,
        options: &VectorIndexOptions<f32>,
        read_ts: u64,
    ) -> Result<Vec<u64>, ChangesetError> {
        let refused = || ChangesetError::NoVectorIndex(predicate.as_str());
        let uid = self
            .quads
            .uid_of(namespace, predicate.as_str())?
            .ok_or_else(refused)?;
        let catalog = self.catalog(namespace)?;
        let spec = catalog
            .predicate(uid)
            .and_then(|d| d.vector_index())
            .ok_or_else(refused)?;
        let graph = match graph {
            GraphNameRef::DefaultGraph => Some(DEFAULT_GRAPH),
            GraphNameRef::NamedNode(n) => self.quads.uid_of(namespace, n.as_str())?,
            GraphNameRef::BlankNode(_) => None,
        };
        let Some(graph) = graph else {
            return Ok(Vec::new());
        };
        let keys = VectorKeys::new(predicate.as_str(), namespace, graph, uid);
        let index = hnsw(spec, keys.clone())?;
        let mut request = self.quads.lists().request(read_ts);
        let txn = ViTxn::new(&mut request);
        let found =
            match index.search_with_options(&QueryCache::new(&txn, read_ts), query, k, options) {
                Err(e) if e.is_empty_index() => return Ok(Vec::new()),
                found => found?,
            };
        // Only the start of a search can lie outside the lists: a node they do not hold is dropped.
        let mut held = Vec::with_capacity(found.len());
        for node in found {
            match Txn::get(&txn, &keys.neighbours(node)) {
                Ok(_) => held.push(node),
                Err(e) if e.is_absence() => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(held)
    }
}

/// The vector of `node` the index at `keys` reads, as `request` sees it; `None` when it holds none.
fn vector_of<S, C>(
    request: &mut Request<'_, S, C>,
    keys: &VectorKeys,
    node: u64,
) -> Result<Option<Vec<f32>>, ChangesetError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    let txn = ViTxn::new(request);
    match Txn::get(&txn, &keys.vector(node)) {
        Ok(bytes) => Ok(bytes_as_float_array::<f32>(&bytes)),
        Err(e) if e.is_absence() => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// The single value of the vector-index list `key` as `request` sees it, `None` when it holds none.
fn index_value<S, C>(
    request: &mut Request<'_, S, C>,
    key: &[u8],
) -> Result<Option<Vec<u8>>, ChangesetError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    let read_ts = request.read_ts();
    let list = request.get(key)?;
    Ok(list
        .static_value(read_ts)
        .and_then(|pl| pl.postings().first())
        .filter(|p| p.op() != DEL)
        .map(|p| p.value().to_vec()))
}

/// The nodes the dead list at `keys` holds, and its value.
fn dead_list<S, C>(
    request: &mut Request<'_, S, C>,
    keys: &VectorKeys,
) -> Result<(Vec<u64>, Option<Vec<u8>>), ChangesetError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    let existing = index_value(request, &keys.dead())?;
    let listed = match &existing {
        Some(v) => parse_edges(&String::from_utf8_lossy(v))?,
        None => Vec::new(),
    };
    Ok((listed, existing))
}

/// Adds `node` to the dead list at `keys`, unless it is listed.
fn mark_dead<S, C>(
    request: &mut Request<'_, S, C>,
    keys: &VectorKeys,
    node: u64,
) -> Result<(), ChangesetError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    let (listed, existing) = dead_list(request, keys)?;
    if listed.contains(&node) {
        return Ok(());
    }
    let dead = add_dead_node(existing.as_deref(), node)?;
    request
        .get(&keys.dead())?
        .add_posting(index_posting(dead), false);
    Ok(())
}

/// Removes `node` from the dead list at `keys`, when it is listed.
fn unmark_dead<S, C>(
    request: &mut Request<'_, S, C>,
    keys: &VectorKeys,
    node: u64,
) -> Result<(), ChangesetError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    let (listed, _) = dead_list(request, keys)?;
    if !listed.contains(&node) {
        return Ok(());
    }
    let kept: Vec<String> = listed
        .iter()
        .filter(|&&u| u != node)
        .map(u64::to_string)
        .collect();
    let value = format!("[{}]", kept.join(",")).into_bytes();
    request
        .get(&keys.dead())?
        .add_posting(index_posting(value), false);
    Ok(())
}
