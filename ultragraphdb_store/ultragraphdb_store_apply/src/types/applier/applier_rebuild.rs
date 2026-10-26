/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/index.go (rebuilder.Run, rebuilder.RunWithoutTemp, BuildIndexes,
 * rebuildTokIndex, rebuildCountIndex, rebuildReverseEdges, BuildData, rebuildListType) and worker/mutation.go
 * (buildIndexes, undoSchemaUpdate). Changed for ultragraphdb: translated to Rust; the indexes are built into a
 * temporary keyspace of the store from the start, Dgraph's temporary Badger, holding no more than a bound of postings
 * in memory, and one batch swaps them in: for every key under a rebuilt prefix the built complete list, or a deletion
 * where the build produced none, at the change's timestamp; for every key under a removed prefix, and under the reverse
 * counts writes kept while both schemas' indexes were maintained, a deletion at the batch's commit timestamp; the
 * list-type rewrite and a `@count` predicate's data lists; the schema record; and the removal of the rebuild record,
 * written and synced before the build began. A failed build deletes, in one batch with the record's removal, every key
 * under a prefix writes maintained that the previous schema does not declare, and restores the previous schema, whose
 * indexes writes kept complete (ledger 39). The temporary keyspace is dropped after the swap or the failure batch. A
 * vector index is built in place, in one request reading each key's merged list, its predicates emptied by the same
 * request, while the change holds every commit back. A list-type rewrite keeps a list's language values; Dgraph's,
 * without `@count`, writes the untagged value alone. A count rebuild lists no subject whose list is empty, where
 * Dgraph's lists it under count 0.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::ops::ControlFlow;

use ultragraphdb_core_keys::{STAR, count_key, index_key, parse, reverse_key};
use ultragraphdb_core_catalog::schema_entry;
use ultragraphdb_core_types::{TypeId, Value, convert, marshal};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_proto_pb::pb::schema_update::Directive;
use ultragraphdb_proto_pb::pb::{DirectedEdge, Posting, SchemaUpdate, directed_edge::Op};
use ultragraphdb_store_posting::{CachedList, List, PostingError, PostingErrorEnum, Request};
use ultragraphdb_traits_store::{
    Entry, Executor, Item, ListCache, Scan, Store, StoreSnapshot, Temporary,
};

use crate::errors::apply_error::{ApplyError, ApplyErrorEnum};
use crate::types::applier::Applier;
use crate::types::build_buffer::{BuildBuffer, DELTAS, SWAP, delta_item, in_region, swap_entry};
use crate::types::index_op::IndexOp;
use crate::types::index_rebuild::IndexRebuild;
use crate::types::index_schema::IndexSchema;
use crate::utils::index::{Maintain, index_vector};
use crate::utils::lock::lock;
use crate::utils::prefixes::{
    attr_data_prefix, attr_reverse_prefix, count_prefixes, declared_prefixes, maintained_prefixes,
    reverse_count_prefixes, reverse_prefixes, tokenizer_prefixes, vector_edge_prefixes,
};

/// What a change rebuilds and removes, by key prefix.
#[derive(Debug, Default)]
struct Plan {
    /// Prefixes whose keys the swap replaces by the built lists.
    rebuilt: Vec<Vec<u8>>,
    /// Prefixes whose keys the swap deletes.
    removed: Vec<Vec<u8>>,
    tokenizers: Vec<String>,
    reverse: bool,
    count: bool,
    /// The reverse counts alone, for reverse edges added to a predicate whose count index stays.
    reverse_count: bool,
    vector: bool,
    list_type: bool,
}

/// What a build produced: the swap's entries in the swap region of its temporary keyspace, when it built any, and a
/// vector index's request.
struct Built<'a, S, C> {
    vector: Option<Request<'a, S, C>>,
    plan: Plan,
}

impl<S, C, E> Applier<S, C, E>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    E: Executor + Send + Sync + 'static,
{
    /// Dgraph's `BuildIndexes` and `BuildData` for `rebuild`, at its timestamp, swapped in at once: the swap's
    /// commit timestamp.
    ///
    /// # Errors
    ///
    /// The build's error, after the previous schema is restored and the keys it rebuilt are deleted.
    pub fn build_indexes(&self, rebuild: &IndexRebuild) -> Result<u64, ApplyError> {
        let _apply = lock(&self.apply);
        let id = self.register_rebuild(
            rebuild.attr(),
            rebuild.old_schema().cloned().unwrap_or_default(),
            rebuild.current_schema().clone(),
        );
        self.lists.pause_rollups();
        if let Err(error) = self.record_rebuild(rebuild) {
            self.abandon(id, rebuild);
            return Err(error);
        }
        self.build_and_swap(id, rebuild)
    }

    /// A build run on the executor: built outside the apply lock, swapped under it, its temporary keyspace dropped
    /// after the swap or the failure batch.
    pub(crate) fn run_job(&self, id: u64, rebuild: IndexRebuild) {
        let mut temporary = None;
        let built = self.build(&rebuild, &mut temporary);
        let _apply = lock(&self.apply);
        let outcome = match built {
            Ok(built) => self.swap(id, &rebuild, built, temporary.as_ref()),
            Err(error) if store_failed(&error) => Err(error),
            Err(error) => self.cleanup(id, &rebuild, &error).and(Err(error)),
        };
        let dropped = self.drop_temporary(temporary, &outcome);
        if let Err(error) = outcome.and(dropped) {
            tracing::error!(attr = rebuild.attr(), %error, "error in building indexes, aborting");
        }
    }

    /// Builds and swaps under the apply lock the caller holds, then drops the temporary keyspace.
    pub(crate) fn build_and_swap(
        &self,
        id: u64,
        rebuild: &IndexRebuild,
    ) -> Result<u64, ApplyError> {
        let mut temporary = None;
        let outcome = match self.build(rebuild, &mut temporary) {
            Ok(built) => self.swap(id, rebuild, built, temporary.as_ref()),
            Err(error) => {
                tracing::error!(attr = rebuild.attr(), %error, "error in building indexes, aborting");
                // A failed write leaves the store to be reopened, which finishes the rebuild by its record.
                if store_failed(&error) {
                    Err(error)
                } else {
                    self.cleanup(id, rebuild, &error).and(Err(error))
                }
            }
        };
        let dropped = self.drop_temporary(temporary, &outcome);
        let ts = outcome?;
        dropped?;
        Ok(ts)
    }

    /// Drops the build's temporary keyspace, if it made one, after the swap or the failure batch: unless a failed
    /// write left the store to be reopened, which drops every temporary keyspace.
    fn drop_temporary(
        &self,
        temporary: Option<Temporary>,
        outcome: &Result<u64, ApplyError>,
    ) -> Result<(), ApplyError> {
        match temporary {
            Some(_) if outcome.as_ref().is_err_and(store_failed) => Ok(()),
            Some(t) => Ok(self.lists.store().drop_temporary(t)?),
            None => Ok(()),
        }
    }

    /// The build: every list of the change's indexes from the data at its timestamp, written to a temporary keyspace
    /// it creates in `temporary` as it goes, so that what it holds in memory stays bounded.
    fn build(
        &self,
        rebuild: &IndexRebuild,
        temporary: &mut Option<Temporary>,
    ) -> Result<Built<'_, S, C>, ApplyError> {
        let attr = rebuild.attr();
        let new = rebuild.current_schema();
        let start_ts = rebuild.start_ts();
        let plan = plan(rebuild)?;
        self.lists.gate(start_ts)?;
        let snapshot = self.lists.store().snapshot(start_ts)?;
        let mut built = Built { vector: None, plan };
        let reverse_counts = (built.plan.count || built.plan.reverse_count)
            && new.directive == Directive::Reverse as i32;
        let tokens_schema = IndexSchema::from_update(
            attr,
            &SchemaUpdate {
                tokenizer: built.plan.tokenizers.clone(),
                index_specs: Vec::new(),
                directive: Directive::Index as i32,
                ..new.clone()
            },
        )?;
        let scans_data = !built.plan.tokenizers.is_empty()
            || built.plan.reverse
            || built.plan.count
            || built.plan.list_type;
        if !scans_data && !reverse_counts {
            if built.plan.vector {
                built.vector = Some(self.build_vector(rebuild, &snapshot)?);
            }
            return Ok(built);
        }
        let store = self.lists.store();
        let max_list_size = self.lists.config().max_list_size();
        let temporary = &*temporary.insert(store.create_temporary()?);
        let mut buffer = BuildBuffer::new(temporary, start_ts);
        if scans_data {
            for item in snapshot.scan(&Scan::prefix(attr_data_prefix(attr)?)) {
                let key = item?.key().to_vec();
                if parse(&key)?.has_start_uid() {
                    continue;
                }
                let (list, _) = List::read(&key, &snapshot)?;
                let uid = parse(&key)?.uid();
                let postings = postings(&list, start_ts)?;
                if !built.plan.tokenizers.is_empty() {
                    for p in &postings {
                        let lang = String::from_utf8_lossy(&p.lang_tag);
                        for token in tokens_schema.tokens(&lang, &posting_val(p))? {
                            buffer.add(index_key(attr, &token)?, uid, p)?;
                        }
                    }
                }
                if built.plan.reverse {
                    for p in &postings {
                        buffer.add(reverse_key(attr, p.uid)?, uid, p)?;
                    }
                }
                if built.plan.count {
                    let size = list.length(start_ts, 0);
                    if size > 0 {
                        buffer.add(
                            count_key(attr, size as u32, false)?,
                            uid,
                            &Posting::default(),
                        )?;
                    }
                }
                if built.plan.list_type || new.count {
                    let rewritten = if built.plan.list_type {
                        re_identified(list, start_ts, &postings, new)?
                    } else {
                        list
                    };
                    buffer.put_swap(complete_entries(rewritten, &key, start_ts, max_list_size)?);
                }
                buffer.maybe_flush(store)?;
            }
        }
        if reverse_counts {
            if built.plan.reverse {
                // The reverse lists the build wrote, merged from the temporary keyspace one at a time.
                buffer.flush(store)?;
                let temp = store.temporary_snapshot(temporary)?;
                let prefix = in_region(DELTAS, &attr_reverse_prefix(attr)?);
                for_each_delta_list(&temp, &prefix, start_ts, |key, list| {
                    add_reverse_count(&mut buffer, attr, &key, &list, start_ts)?;
                    buffer.maybe_flush(store)
                })?;
            } else {
                for item in snapshot.scan(&Scan::prefix(attr_reverse_prefix(attr)?)) {
                    let key = item?.key().to_vec();
                    if parse(&key)?.has_start_uid() {
                        continue;
                    }
                    let (list, _) = List::read(&key, &snapshot)?;
                    add_reverse_count(&mut buffer, attr, &key, &list, start_ts)?;
                    if new.count {
                        buffer.put_swap(complete_entries(list, &key, start_ts, max_list_size)?);
                    }
                    buffer.maybe_flush(store)?;
                }
            }
        }
        buffer.flush(store)?;
        // Every delta list merged and rolled up into the swap's entries, one list at a time.
        let temp = store.temporary_snapshot(temporary)?;
        for_each_delta_list(&temp, &[DELTAS], start_ts, |key, list| {
            buffer.put_swap(complete_entries(list, &key, start_ts, max_list_size)?);
            buffer.maybe_flush(store)
        })?;
        buffer.flush(store)?;
        tracing::debug!(
            attr,
            postings = buffer.added(),
            postings_held = buffer.peak(),
            "index built into the temporary keyspace"
        );
        if built.plan.vector {
            built.vector = Some(self.build_vector(rebuild, &snapshot)?);
        }
        Ok(built)
    }

    /// Dgraph's `RunWithoutTemp` for a vector index: one request at the change's timestamp empties the predicates the
    /// index keeps, then inserts every vector, a value of another type converted to a vector first. Each key's list is
    /// read merged, where Dgraph reads its newest version alone.
    fn build_vector(
        &self,
        rebuild: &IndexRebuild,
        snapshot: &S::Snapshot,
    ) -> Result<Request<'_, S, C>, ApplyError> {
        let attr = rebuild.attr();
        let start_ts = rebuild.start_ts();
        let mut request = self.lists.request(start_ts);
        let delete_all = DirectedEdge {
            value: STAR.as_bytes().to_vec(),
            op: Op::Del as i32,
            ..DirectedEdge::default()
        };
        for prefix in vector_edge_prefixes(attr)? {
            for item in snapshot.scan(&Scan::prefix(prefix)) {
                let key = item?.key().to_vec();
                if parse(&key)?.has_start_uid() {
                    continue;
                }
                request.get(&key)?.add_mutation(&delete_all, None)?;
            }
        }
        let schema = IndexSchema::from_update(attr, rebuild.current_schema())?;
        let posting_schema = schema.posting_schema();
        let keys: Vec<Vec<u8>> = lists_under(snapshot, &attr_data_prefix(attr)?)?
            .into_iter()
            .map(|(k, _)| k)
            .collect();
        for key in keys {
            let uid = parse(&key)?.uid();
            let val = request.get(&key)?.value(start_ts)?;
            let vector = if val.tid() == TypeId::VFLOAT {
                bytes_of(val.value())
            } else {
                let b = marshal(&convert(&val, TypeId::VFLOAT)?, TypeId::BINARY)?;
                let bytes = bytes_of(b.value());
                let edge = DirectedEdge {
                    entity: uid,
                    attr: attr.to_string(),
                    value: bytes.clone(),
                    value_type: TypeId::VFLOAT.enum_value(),
                    op: Op::Set as i32,
                    ..DirectedEdge::default()
                };
                request
                    .get(&key)?
                    .add_mutation(&edge, posting_schema.as_ref())?;
                bytes
            };
            let edge = DirectedEdge {
                entity: uid,
                attr: attr.to_string(),
                value: vector,
                value_type: TypeId::VFLOAT.enum_value(),
                op: Op::Set as i32,
                ..DirectedEdge::default()
            };
            let mut draw = || self.draw();
            let mut m = Maintain {
                schema: &schema,
                indexing: true,
                draw: &mut draw,
            };
            index_vector(&mut request, &mut m, &edge)?;
        }
        Ok(request)
    }

    /// The swap batch, written under the apply lock the caller holds: the swap's commit timestamp, 0 when a drop
    /// cancelled the rebuild.
    fn swap(
        &self,
        id: u64,
        rebuild: &IndexRebuild,
        built: Built<'_, S, C>,
        temporary: Option<&Temporary>,
    ) -> Result<u64, ApplyError> {
        let attr = rebuild.attr();
        if !self.still_rebuilding(attr, id) {
            tracing::info!(attr, "rebuild discarded: its predicate was dropped");
            self.lists.resume_rollups();
            self.remove_record(rebuild)?;
            return Ok(0);
        }
        let start_ts = rebuild.start_ts();
        let new = rebuild.current_schema().clone();
        let latest = self.lists.latest_ts();
        self.lists.gate(latest)?;
        let snapshot = self.lists.store().snapshot(latest)?;
        let Built { vector, plan } = built;
        let mut request = vector.unwrap_or_else(|| self.lists.request(latest));
        let temp = match temporary {
            Some(t) => Some(self.lists.store().temporary_snapshot(t)?),
            None => None,
        };
        if let Some(temp) = &temp {
            for item in temp.scan(&Scan::prefix(vec![SWAP])) {
                request.add_entry_at_version(swap_entry(item?));
            }
        }
        // Every prefix writes maintained that the new schema does not declare is emptied at the batch's commit
        // timestamp, above every write that maintained it: the removed indexes, and the reverse counts the two
        // schemas' indexes kept together.
        let declared = declared_prefixes(attr, &new)?;
        let mut emptied = plan.removed.clone();
        for p in maintained_prefixes(attr, rebuild.old_schema(), &new)?
            .into_iter()
            .chain(plan.rebuilt.iter().cloned())
        {
            if !declared.contains(&p) && !emptied.contains(&p) {
                emptied.push(p);
            }
        }
        for prefix in plan.rebuilt.iter().filter(|p| declared.contains(p)) {
            for item in snapshot.scan(&Scan::prefix(prefix.clone())) {
                let key = item?.key().to_vec();
                let written = match &temp {
                    Some(temp) => temp.get(&in_region(SWAP, &key))?.is_some(),
                    None => false,
                };
                if !written {
                    request.add_entry_at_version(Entry::deletion(key, start_ts));
                }
            }
        }
        for prefix in &emptied {
            for item in snapshot.scan(&Scan::prefix(prefix.clone())) {
                request.add_entry(Entry::deletion(item?.key().to_vec(), 0));
            }
        }
        request.add_entry(schema_entry(&new, 0)?);
        request.remove_rebuild_record_on_commit(start_ts);
        request.advance_epoch_on_commit();
        let mut batch = self.lists.batch();
        batch.push(request);
        let results = self.lists.commit_with(batch, |_| {
            self.schema.set(attr, new.clone());
            self.schema.delete_mut_schema(attr);
            self.end_rebuild(attr, id);
            Ok::<_, ApplyError>(())
        });
        self.lists.resume_rollups();
        let ts = crate::types::applier::applier_mutations::first_result(results?)?;
        self.fence(emptied, ts);
        tracing::info!(attr, "Done schema update");
        Ok(ts)
    }

    /// Undoes a failed build under the apply lock the caller holds: restores the previous schema and deletes, in one
    /// batch, every key under a rebuilt prefix the previous schema does not declare.
    fn cleanup(
        &self,
        id: u64,
        rebuild: &IndexRebuild,
        error: &ApplyError,
    ) -> Result<(), ApplyError> {
        let attr = rebuild.attr();
        if !self.end_rebuild(attr, id) {
            self.lists.resume_rollups();
            return self.remove_record(rebuild);
        }
        tracing::error!(attr, %error, "error in building indexes, aborting");
        // Dgraph's undoSchemaUpdate reloads the previous schema record, which the swap never replaced.
        if let Some(old) = rebuild.old_schema() {
            self.schema.set(attr, old.clone());
        }
        self.schema.delete_mut_schema(attr);
        let declared = match rebuild.old_schema() {
            Some(old) => declared_prefixes(attr, old)?,
            None => Vec::new(),
        };
        let undeclared: Vec<Vec<u8>> =
            maintained_prefixes(attr, rebuild.old_schema(), rebuild.current_schema())?
                .into_iter()
                .chain(plan(rebuild)?.rebuilt)
                .filter(|p| !declared.contains(p))
                .fold(Vec::new(), |mut acc, p| {
                    if !acc.contains(&p) {
                        acc.push(p);
                    }
                    acc
                });
        let outcome = self.delete_under(&undeclared, Some(rebuild.start_ts()));
        self.lists.resume_rollups();
        let ts = outcome?;
        if ts > 0 {
            self.fence(undeclared, ts);
        }
        Ok(())
    }

    /// Deletes every key under `prefixes` and removes the rebuild record `record`, in one batch at its commit
    /// timestamp: the timestamp, 0 when there is nothing to write.
    pub(crate) fn delete_under(
        &self,
        prefixes: &[Vec<u8>],
        record: Option<u64>,
    ) -> Result<u64, ApplyError> {
        let latest = self.lists.latest_ts();
        self.lists.gate(latest)?;
        let snapshot = self.lists.store().snapshot(latest)?;
        let mut request = self.lists.request(latest);
        let mut any = false;
        for prefix in prefixes {
            for item in snapshot.scan(&Scan::prefix(prefix.clone())) {
                request.add_entry(Entry::deletion(item?.key().to_vec(), 0));
                any = true;
            }
        }
        if let Some(id) = record {
            request.remove_rebuild_record_on_commit(id);
        } else if !any {
            return Ok(0);
        }
        request.advance_epoch_on_commit();
        let mut batch = self.lists.batch();
        batch.push(request);
        Ok(self.lists.commit(batch)?.into_iter().next().unwrap_or(0))
    }
}

/// Whether `error` is a failed write of the store, which recovers only by reopening.
fn store_failed(error: &ApplyError) -> bool {
    matches!(
        &error.0,
        ApplyErrorEnum::Posting(PostingError(PostingErrorEnum::Store(StoreError(
            StoreErrorEnum::WriteFailed(_) | StoreErrorEnum::Poisoned
        ))))
    )
}

/// What `rebuild` rebuilds and removes.
fn plan(rebuild: &IndexRebuild) -> Result<Plan, ApplyError> {
    let attr = rebuild.attr();
    let info = rebuild.needs_tok_index_rebuild()?;
    let mut plan = Plan::default();
    match info.op() {
        IndexOp::Rebuild => {
            plan.rebuilt.extend(info.rebuilt_tokenizer_prefixes()?);
            plan.removed.extend(info.removed_tokenizer_prefixes()?);
            plan.tokenizers = info.tokenizers_to_rebuild().to_vec();
            plan.vector = !info.vector_indexes_to_rebuild().is_empty();
        }
        IndexOp::Delete => {
            for t in info.tokenizers_to_delete() {
                plan.removed.extend(tokenizer_prefixes(attr, t)?);
            }
        }
        IndexOp::Noop => {}
    }
    for spec in info.vector_indexes_to_delete() {
        plan.removed.extend(tokenizer_prefixes(attr, &spec.name)?);
    }
    // Dgraph builds the reverse counts only with the count index, so reverse edges added to a `@count` predicate
    // get none and removed from one leave theirs; the port builds and removes them with the reverse edges.
    let count_stays = rebuild.needs_count_index_rebuild() == IndexOp::Noop;
    match rebuild.needs_reverse_edges_rebuild() {
        IndexOp::Rebuild => {
            plan.reverse = true;
            plan.rebuilt.extend(reverse_prefixes(attr)?);
            if count_stays && rebuild.current_schema().count {
                plan.reverse_count = true;
                plan.rebuilt.extend(reverse_count_prefixes(attr)?);
            }
        }
        IndexOp::Delete => {
            plan.removed.extend(reverse_prefixes(attr)?);
            if count_stays && rebuild.old_schema().is_some_and(|o| o.count) {
                plan.removed.extend(reverse_count_prefixes(attr)?);
            }
        }
        IndexOp::Noop => {}
    }
    match rebuild.needs_count_index_rebuild() {
        IndexOp::Rebuild => {
            plan.count = true;
            plan.rebuilt.extend(count_prefixes(attr)?);
        }
        IndexOp::Delete => plan.removed.extend(count_prefixes(attr)?),
        IndexOp::Noop => {}
    }
    if rebuild.needs_vector_index_edges_rebuild()? == IndexOp::Delete {
        plan.removed.extend(vector_edge_prefixes(attr)?);
    }
    plan.list_type = rebuild.needs_list_type_rebuild()?;
    Ok(plan)
}

/// A posting's value as stored, untyped.
fn posting_val(p: &Posting) -> ultragraphdb_core_types::Val {
    ultragraphdb_core_types::Val::new(TypeId::new(p.val_type), Value::Bytes(p.value.clone()))
}

fn bytes_of(v: Option<&Value>) -> Vec<u8> {
    match v {
        Some(Value::Bytes(b)) => b.clone(),
        _ => Vec::new(),
    }
}

/// The postings of `list` at `read_ts`.
fn postings(list: &List, read_ts: u64) -> Result<Vec<Posting>, ApplyError> {
    let mut out = Vec::new();
    list.iterate(read_ts, 0, |p| {
        out.push(p.clone());
        Ok::<_, PostingError>(ControlFlow::Continue(()))
    })?;
    Ok(out)
}

/// The lists under `prefix` at `snapshot`, each read with its parts, split parts left out as keys.
fn lists_under<T: StoreSnapshot>(
    snapshot: &T,
    prefix: &[u8],
) -> Result<Vec<(Vec<u8>, List)>, ApplyError> {
    let mut out = Vec::new();
    for item in snapshot.scan(&Scan::prefix(prefix.to_vec())) {
        let key = item?.key().to_vec();
        if parse(&key)?.has_start_uid() {
            continue;
        }
        let (list, _) = List::read(&key, snapshot)?;
        out.push((key, list));
    }
    Ok(out)
}

/// Calls `f` with every list whose deltas lie under `prefix` of the temporary keyspace `temp`, one list at a time,
/// in key order: each list merged from its deltas and built again as one written at `start_ts`, its postings in its
/// scratch layer, as the lists the build added to were.
fn for_each_delta_list<T, F>(
    temp: &T,
    prefix: &[u8],
    start_ts: u64,
    mut f: F,
) -> Result<(), ApplyError>
where
    T: StoreSnapshot,
    F: FnMut(Vec<u8>, List) -> Result<(), ApplyError>,
{
    let mut versions: Vec<Item> = Vec::new();
    let mut finish = |versions: &mut Vec<Item>| -> Result<(), ApplyError> {
        let Some(first) = versions.first() else {
            return Ok(());
        };
        let key = first.key().to_vec();
        let read = std::mem::take(versions).into_iter().map(Ok);
        let (merged, _) = List::read_versions(&key, read)?;
        let mut list = List::empty(key.clone());
        list.set_ts(start_ts);
        for p in postings(&merged, u64::MAX)? {
            let edge = DirectedEdge {
                value_id: p.uid,
                op: Op::Set as i32,
                facets: p.facets,
                ..DirectedEdge::default()
            };
            list.add_mutation(&edge, None)?;
        }
        f(key, list)
    };
    for item in temp.scan(&Scan::prefix(prefix.to_vec()).all_versions()) {
        let item = delta_item(item?);
        if versions.first().is_some_and(|v| v.key() != item.key()) {
            finish(&mut versions)?;
        }
        versions.push(item);
    }
    finish(&mut versions)
}

/// Adds `key`'s reverse list, `list`, to the reverse count index under its length.
fn add_reverse_count(
    buffer: &mut BuildBuffer<'_>,
    attr: &str,
    key: &[u8],
    list: &List,
    start_ts: u64,
) -> Result<(), ApplyError> {
    let target = parse(key)?.uid();
    let size = list.length(start_ts, 0);
    if size > 0 {
        buffer.add(
            count_key(attr, size as u32, true)?,
            target,
            &Posting::default(),
        )?;
    }
    Ok(())
}

/// Dgraph's `rebuildListType`: the list with its untagged value re-identified as a list value, its language values
/// kept.
fn re_identified(
    mut list: List,
    start_ts: u64,
    postings: &[Posting],
    new: &SchemaUpdate,
) -> Result<List, ApplyError> {
    let Some(untagged) = postings.iter().find(|p| p.uid == u64::MAX) else {
        return Ok(list);
    };
    list.set_ts(start_ts);
    let del = DirectedEdge {
        value_id: untagged.uid,
        op: Op::Del as i32,
        ..DirectedEdge::default()
    };
    let schema = SchemaUpdate {
        value_type: new.value_type,
        list: true,
        ..SchemaUpdate::default()
    };
    list.add_mutation(&del, Some(&schema))?;
    let set = DirectedEdge {
        value: untagged.value.clone(),
        value_type: untagged.val_type,
        op: Op::Set as i32,
        facets: untagged.facets.clone(),
        ..DirectedEdge::default()
    };
    list.add_mutation(&set, Some(&schema))?;
    Ok(list)
}

/// The writes of `list`, whose scratch layer holds what the build added, as a complete version at `start_ts`.
fn complete_entries(
    mut list: List,
    key: &[u8],
    start_ts: u64,
    max_list_size: usize,
) -> Result<Vec<Entry>, ApplyError> {
    let old_splits = list.part_splits();
    list.commit_mutation(start_ts);
    let Some(out) = list.rollup(u64::MAX, true, max_list_size)? else {
        return Ok(Vec::new());
    };
    Ok(out.entries(key, &old_splits, start_ts)?)
}
