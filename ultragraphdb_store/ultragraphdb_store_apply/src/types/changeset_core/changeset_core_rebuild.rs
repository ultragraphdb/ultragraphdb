/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A build of a predicate's indexes. It reads every forward and reverse list of the predicate at the change's
//! timestamp into a temporary keyspace, holding no more than a bound of postings in memory, merges each list built,
//! and swaps them in with one batch under the apply lock: each built list complete at the change's timestamp, a
//! deletion there for each key under a rebuilt prefix the build produced none for, and the new declaration. The keys
//! of the indexes the change removes are then dropped physically, and the record removed; a crash in between leaves
//! the record, whose indexes the new declaration does not name, to be finished at open. A build that fails undoes the
//! change: the old declaration back, then the keys of the indexes it built dropped and the record removed; the old
//! declaration's indexes, which writes kept, stay complete.
//!
//! A vector index is built afresh in memory, one per graph, its inserts reading each node's vector at the change's
//! timestamp. Writes during the build leave it alone and record the nodes whose vector they change; the swap inserts
//! their vectors, or marks them dead, then writes the built lists over the predicate's vector-index keys, deleting
//! those the build did not write.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;

use oxrdf::Term;
use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_codec::Posting;
use ultragraphdb_core_keys::{CountDirection, Keyspace, QuadKey};
use ultragraphdb_core_mapping::Declaration;
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_hnsw::{VectorKeys, add_dead_node, bytes_as_float_array, index_posting};
use ultragraphdb_store_posting::{CachedList, List, PostingError, PostingErrorEnum, SET};
use ultragraphdb_store_quads::object_posting;
use ultragraphdb_traits_index::{Txn, VectorIndex};
use ultragraphdb_traits_store::{Entry, Item, ListCache, Scan, Store, StoreSnapshot, Temporary};

use crate::errors::changeset_error::{ChangesetError, ChangesetErrorEnum};
use crate::types::build_buffer::{BuildBuffer, DELTAS, SWAP, delta_item, swap_entry};
use crate::types::changeset_core::ChangesetCore;
use crate::types::overlay_txn::OverlayTxn;
use crate::types::predicate_prefix::PredicatePrefix;
use crate::types::rebuild::Rebuild;
use crate::utils::literal_tokens::literal_tokens;
use crate::utils::lock::lock;
use crate::utils::quad_prefixes::{count_prefixes, tokenizer_prefixes, vector_prefixes};
use crate::utils::vector_index::hnsw;

/// The vector-index lists a build made, by graph, then by key.
type BuiltVectors = BTreeMap<u64, BTreeMap<Vec<u8>, Vec<u8>>>;

impl<S, C> ChangesetCore<S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// Registers `rebuild`, whose change committed, and pauses rollups until it ends: its record's number.
    pub(super) fn begin(&self, rebuild: Rebuild) -> u64 {
        let record = rebuild.record;
        *lock(&self.rebuild) = Some(rebuild);
        self.quads.lists().pause_rollups();
        record
    }

    /// The build of record `record`, while it runs.
    fn current(&self, record: u64) -> Option<Rebuild> {
        lock(&self.rebuild).clone().filter(|r| r.record == record)
    }

    /// Adds `touched`, the subjects whose vector a write under `running` changed, to the build.
    pub(super) fn touch(&self, running: &Rebuild, touched: BTreeSet<(u64, u64)>) {
        if let Some(r) = lock(&self.rebuild)
            .as_mut()
            .filter(|r| r.record == running.record)
        {
            r.touched.extend(touched);
        }
    }

    /// Ends the build when `cancel` selects it, as a drop of its predicate, data or namespace does: the build, now
    /// cancelled; its job removes its record.
    pub(super) fn cancel(&self, cancel: impl Fn(&Rebuild) -> bool) -> Option<Rebuild> {
        let mut rebuild = lock(&self.rebuild);
        if !rebuild.as_ref().is_some_and(cancel) {
            return None;
        }
        self.quads.lists().resume_rollups();
        rebuild.take()
    }

    /// The build of record `record`, run on the executor: built outside the apply lock, swapped, or undone, under it,
    /// its temporary keyspace dropped after.
    pub fn run_rebuild(&self, record: u64) {
        let Some(rebuild) = self.current(record) else {
            // A drop cancelled the build before it ran: its job removes its record.
            let _apply = lock(&self.apply);
            tracing::info!(record, "build discarded: a drop cancelled it");
            if let Err(error) = self.remove_record(record) {
                tracing::error!(record, %error, "error in building indexes, aborting");
            }
            return;
        };
        let mut temporary = None;
        let built = self.build(&rebuild, &mut temporary);
        let _apply = lock(&self.apply);
        let outcome = match built {
            Ok(vectors) => self.swap(record, temporary.as_ref(), vectors),
            Err(error) if store_failed(&error) => Err(error),
            Err(error) => self.undo(record).and(Err(error)),
        };
        let failed_write = outcome.as_ref().is_err_and(store_failed);
        if let Some(t) = temporary.filter(|_| !failed_write)
            && let Err(error) = self.quads.store().drop_temporary(t)
        {
            tracing::error!(record, %error, "the build's temporary keyspace could not be dropped");
        }
        if let Err(error) = outcome {
            tracing::error!(record, %error, "error in building indexes, aborting");
        }
    }

    /// The lists `rebuild` builds, from every list of its predicate at its timestamp: the value-index and count lists
    /// written to a temporary keyspace it creates in `temporary`, each merged list's complete entries in its swap
    /// region; the vector index's lists, returned.
    fn build(
        &self,
        rebuild: &Rebuild,
        temporary: &mut Option<Temporary>,
    ) -> Result<BuiltVectors, ChangesetError> {
        let vectors = self.build_vectors(rebuild)?;
        let plan = &rebuild.plan;
        if plan.build_tokenizers.is_empty() && !plan.build_count {
            return Ok(vectors);
        }
        let (ns, predicate, start_ts) = (rebuild.namespace, rebuild.predicate, rebuild.start_ts);
        let lists = self.quads.lists();
        lists.gate(start_ts)?;
        let snapshot = lists.store().snapshot(start_ts)?;
        let store = lists.store();
        let temp = &*temporary.insert(store.create_temporary()?);
        let mut buffer = BuildBuffer::new(temp, start_ts);
        let fp = self.quads.fingerprint();
        let count_key = |graph, direction, count: usize| {
            QuadKey::CountIndex {
                namespace: ns,
                graph,
                predicate,
                direction,
                count: u32::try_from(count).unwrap_or(u32::MAX),
            }
            .to_bytes()
        };
        let forward = PredicatePrefix::new(Keyspace::Forward, false, ns, predicate, Vec::new());
        let reverse = PredicatePrefix::new(Keyspace::Reverse, false, ns, predicate, Vec::new());
        for graph in forward.graphs(&snapshot)? {
            for item in snapshot.scan(&Scan::prefix(forward.in_graph(graph))) {
                let key = item?.key().to_vec();
                let Ok((QuadKey::Forward { subject, .. }, None)) = QuadKey::parse(&key) else {
                    continue;
                };
                let objects = self.quads.objects(&key, start_ts)?;
                for (object, _) in objects.iter().filter(|(o, _)| o.as_uid().is_none()) {
                    if plan.build_tokenizers.is_empty() {
                        break;
                    }
                    let Term::Literal(literal) = self.quads.term_of(ns, object)? else {
                        continue;
                    };
                    for (tokenizer, token) in
                        literal_tokens(&plan.build_tokenizers, literal.as_ref())?
                    {
                        let key = QuadKey::ValueIndex {
                            namespace: ns,
                            graph,
                            predicate,
                            tokenizer,
                            token,
                        }
                        .to_bytes();
                        buffer.add_posting(key, ref_posting(subject));
                    }
                }
                if plan.build_count && !objects.is_empty() {
                    let key = count_key(graph, CountDirection::Forward, objects.len());
                    buffer.add_posting(key, ref_posting(subject));
                }
                buffer.maybe_flush(store)?;
            }
            if plan.build_count {
                for item in snapshot.scan(&Scan::prefix(reverse.in_graph(graph))) {
                    let key = item?.key().to_vec();
                    let Ok((QuadKey::Reverse { object, .. }, None)) = QuadKey::parse(&key) else {
                        continue;
                    };
                    let subjects = self.quads.objects(&key, start_ts)?.len();
                    if subjects > 0 {
                        let key = count_key(graph, CountDirection::Reverse, subjects);
                        buffer.add_posting(key, object_posting(&object, SET, &[], fp));
                    }
                    buffer.maybe_flush(store)?;
                }
            }
        }
        buffer.flush(store)?;
        let max_list_size = lists.config().max_list_size();
        let temp_snapshot = store.temporary_snapshot(temp)?;
        for_each_built_list(&temp_snapshot, start_ts, |key, list| {
            buffer.put_swap(complete_entries(list, &key, start_ts, max_list_size)?);
            Ok(buffer.maybe_flush(store)?)
        })?;
        buffer.flush(store)?;
        tracing::debug!(
            predicate,
            postings = buffer.added(),
            postings_held = buffer.peak(),
            "indexes built into the temporary keyspace"
        );
        Ok(vectors)
    }

    /// The vector index `rebuild` builds, one per graph holding its predicate: every node's vector inserted in key
    /// order, read at the build's timestamp.
    fn build_vectors(&self, rebuild: &Rebuild) -> Result<BuiltVectors, ChangesetError> {
        let mut built = BuiltVectors::new();
        let Some(spec) = rebuild
            .new
            .vector_index()
            .filter(|_| rebuild.plan.build_vector)
        else {
            return Ok(built);
        };
        let (ns, predicate, start_ts) = (rebuild.namespace, rebuild.predicate, rebuild.start_ts);
        let lists = self.quads.lists();
        lists.gate(start_ts)?;
        let snapshot = lists.store().snapshot(start_ts)?;
        let forward = PredicatePrefix::new(Keyspace::Forward, false, ns, predicate, Vec::new());
        let name = rebuild.new.predicate().as_str();
        for graph in forward.graphs(&snapshot)? {
            let keys = VectorKeys::new(name, ns, graph, predicate);
            let index = hnsw(spec, keys.clone())?;
            let txn = OverlayTxn::new(&self.quads, keys, start_ts, BTreeMap::new());
            for item in snapshot.scan(&Scan::prefix(forward.in_graph(graph))) {
                let key = item?.key().to_vec();
                let Some(node) = txn.keys().node_of(&key) else {
                    continue;
                };
                self.insert_built(&index, &txn, node, false)?;
            }
            built.insert(graph, txn.into_values());
        }
        Ok(built)
    }

    /// Inserts `node`'s vector into the index `txn` builds; when it holds no vector, marks it dead if
    /// `dead_when_absent` holds, as a node whose vector a write deleted during the build is. The built index's dead
    /// list is written here alone, by the swap, once per node it touched: the index starts empty, its inserts only read
    /// the dead list, and the build marks no node dead. So no node inserted is listed dead, and no node marked dead
    /// is listed already.
    fn insert_built(
        &self,
        index: &ultragraphdb_store_hnsw::PersistentHnsw<f32>,
        txn: &OverlayTxn<'_, S, C>,
        node: u64,
        dead_when_absent: bool,
    ) -> Result<(), ChangesetError> {
        let keys = txn.keys().clone();
        match txn.get(&keys.vector(node)) {
            Ok(bytes) => {
                let Some(vector) = bytes_as_float_array::<f32>(&bytes) else {
                    return Ok(());
                };
                let mut draw = || lock(&self.draw).next_f64();
                VectorIndex::insert(index, txn, node, &vector, &mut draw)?;
            }
            Err(e) if e.is_absence() => {
                if dead_when_absent {
                    let dead_key = keys.dead();
                    let existing = txn.get(&dead_key).ok();
                    txn.write(&dead_key, &add_dead_node(existing.as_deref(), node)?);
                }
            }
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }

    /// The swap batch of record `record`, under the apply lock the caller holds: its commit timestamp, 0 when a drop
    /// cancelled the build.
    fn swap(
        &self,
        record: u64,
        temporary: Option<&Temporary>,
        mut vectors: BuiltVectors,
    ) -> Result<u64, ChangesetError> {
        let Some(rebuild) = self.current(record) else {
            tracing::info!(record, "build discarded: a drop cancelled it");
            self.remove_record(record)?;
            return Ok(0);
        };
        let (ns, predicate, start_ts) = (rebuild.namespace, rebuild.predicate, rebuild.start_ts);
        let plan = &rebuild.plan;
        let lists = self.quads.lists();
        let latest = lists.latest_ts();
        lists.gate(latest)?;
        let snapshot = lists.store().snapshot(latest)?;
        let mut request = lists.request(latest);
        let mut built: BTreeSet<Vec<u8>> = BTreeSet::new();
        if let Some(t) = temporary {
            let temp = lists.store().temporary_snapshot(t)?;
            for item in temp.scan(&Scan::prefix(vec![SWAP])) {
                let entry = swap_entry(item?);
                built.insert(entry.key().to_vec());
                request.add_entry_at_version(entry);
            }
        }
        let mut rebuilt: Vec<PredicatePrefix> = plan
            .build_tokenizers
            .iter()
            .flat_map(|t| tokenizer_prefixes(ns, predicate, t))
            .collect();
        if plan.build_count {
            rebuilt.extend(count_prefixes(ns, predicate));
        }
        for key in keys_under(&snapshot, &rebuilt)? {
            if !built.contains(&key) {
                request.add_entry_at_version(Entry::deletion(key, start_ts));
            }
        }
        let mut removed: Vec<PredicatePrefix> = plan
            .remove_tokenizers
            .iter()
            .flat_map(|t| tokenizer_prefixes(ns, predicate, t))
            .collect();
        if plan.remove_count {
            removed.extend(count_prefixes(ns, predicate));
        }
        if plan.remove_vector {
            removed.extend(vector_prefixes(ns, predicate));
        }
        if let Some(spec) = rebuild.new.vector_index().filter(|_| plan.build_vector) {
            let name = rebuild.new.predicate().as_str();
            for (graph, node) in &rebuild.touched {
                let keys = VectorKeys::new(name, ns, *graph, predicate);
                let index = hnsw(spec, keys.clone())?;
                let values = vectors.remove(graph).unwrap_or_default();
                let txn = OverlayTxn::new(&self.quads, keys, latest, values);
                self.insert_built(&index, &txn, *node, true)?;
                vectors.insert(*graph, txn.into_values());
            }
            let mut written: BTreeSet<Vec<u8>> = BTreeSet::new();
            for (key, value) in vectors.into_values().flatten() {
                request.get(&key)?.add_posting(index_posting(value), false);
                written.insert(key);
            }
            for key in keys_under(&snapshot, &vector_prefixes(ns, predicate))? {
                if !written.contains(&key) {
                    request.add_entry(Entry::deletion(key, 0));
                }
            }
        }
        let declaration = Declaration::Predicate(rebuild.new.clone());
        request.add_entry(Catalog::entry(ns, predicate, &declaration, 0));
        request.advance_epoch_on_commit();
        let mut batch = lists.batch();
        batch.push(request);
        let ts = lists.commit(batch)?.into_iter().next().unwrap_or(0);
        let mut catalog = (*self.catalog(ns)?).clone();
        catalog.declare_predicate(predicate, rebuild.new.clone())?;
        lock(&self.catalogs).insert(ns, std::sync::Arc::new(catalog));
        self.cancel(|r| r.record == record);
        self.drop_removed(&removed)?;
        self.remove_record(record)?;
        tracing::info!(record, ts, "indexes built and swapped in");
        Ok(ts)
    }

    /// Undoes the change of record `record` after its build failed, under the apply lock the caller holds: writes the
    /// old declaration back, then drops the keys of the indexes the build made, which it does not name, and removes
    /// the record.
    fn undo(&self, record: u64) -> Result<(), ChangesetError> {
        let Some(rebuild) = self.cancel(|r| r.record == record) else {
            return self.remove_record(record);
        };
        let (ns, predicate) = (rebuild.namespace, rebuild.predicate);
        let plan = &rebuild.plan;
        let mut built: Vec<PredicatePrefix> = plan
            .build_tokenizers
            .iter()
            .flat_map(|t| tokenizer_prefixes(ns, predicate, t))
            .collect();
        if plan.build_count {
            built.extend(count_prefixes(ns, predicate));
        }
        let lists = self.quads.lists();
        let mut request = lists.request(lists.latest_ts());
        let declaration = Declaration::Predicate(rebuild.old.clone());
        request.add_entry(Catalog::entry(ns, predicate, &declaration, 0));
        request.advance_epoch_on_commit();
        let mut batch = lists.batch();
        batch.push(request);
        lists.commit(batch)?;
        let mut catalog = (*self.catalog(ns)?).clone();
        catalog.declare_predicate(predicate, rebuild.old.clone())?;
        lock(&self.catalogs).insert(ns, std::sync::Arc::new(catalog));
        self.drop_removed(&built)?;
        self.remove_record(record)
    }
}

/// A subject's posting in a value-index or forward count list.
fn ref_posting(uid: u64) -> Posting {
    Posting::default().with_uid(uid).with_op(SET)
}

/// Every key `snapshot` holds under `prefixes`, in every graph.
fn keys_under<T: StoreSnapshot>(
    snapshot: &T,
    prefixes: &[PredicatePrefix],
) -> Result<Vec<Vec<u8>>, ChangesetError> {
    let mut keys = Vec::new();
    for prefix in prefixes {
        for graph_prefix in prefix.expand(snapshot)? {
            for item in snapshot.scan(&Scan::prefix(graph_prefix)) {
                keys.push(item?.key().to_vec());
            }
        }
    }
    Ok(keys)
}

/// Whether `error` is a failed write of the store, which recovers only by reopening.
fn store_failed(error: &ChangesetError) -> bool {
    let failed = |e: &StoreError| {
        matches!(
            e.0,
            StoreErrorEnum::WriteFailed(_) | StoreErrorEnum::Poisoned
        )
    };
    match &error.0 {
        ChangesetErrorEnum::Store(e) => failed(e),
        ChangesetErrorEnum::Posting(PostingError(PostingErrorEnum::Store(e))) => failed(e),
        _ => false,
    }
}

/// Calls `f` with every list whose deltas the build wrote to the delta region of `temp`, one list at a time, in key
/// order: each merged from its deltas and built again as one written at `start_ts`, its postings in its scratch layer.
fn for_each_built_list<T, F>(temp: &T, start_ts: u64, mut f: F) -> Result<(), ChangesetError>
where
    T: StoreSnapshot,
    F: FnMut(Vec<u8>, List) -> Result<(), ChangesetError>,
{
    let mut versions: Vec<Item> = Vec::new();
    let mut finish = |versions: &mut Vec<Item>| -> Result<(), ChangesetError> {
        let Some(first) = versions.first() else {
            return Ok(());
        };
        let key = first.key().to_vec();
        let read = std::mem::take(versions).into_iter().map(Ok);
        let (merged, _) = List::read_versions(&key, read)?;
        let mut list = List::empty(key.clone());
        list.set_ts(start_ts);
        merged.iterate(u64::MAX, 0, |p| {
            list.add_posting(p.clone(), false);
            Ok::<_, PostingError>(ControlFlow::Continue(()))
        })?;
        f(key, list)
    };
    for item in temp.scan(&Scan::prefix(vec![DELTAS]).all_versions()) {
        let item = delta_item(item?);
        if versions.first().is_some_and(|v| v.key() != item.key()) {
            finish(&mut versions)?;
        }
        versions.push(item);
    }
    finish(&mut versions)
}

/// The writes of `list`, whose scratch layer holds what the build added, as a complete version at `start_ts`.
fn complete_entries(
    mut list: List,
    key: &[u8],
    start_ts: u64,
    max_list_size: usize,
) -> Result<Vec<Entry>, PostingError> {
    let old_splits = list.part_splits();
    list.commit_mutation(start_ts);
    let Some(out) = list.rollup(u64::MAX, true, max_list_size)? else {
        return Ok(Vec::new());
    };
    out.entries(key, &old_splits, start_ts)
}
