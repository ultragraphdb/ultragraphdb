/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/index.go (rebuilder.Run, rebuilder.RunWithoutTemp, BuildIndexes,
 * rebuildTokIndex, rebuildCountIndex, rebuildReverseEdges, BuildData, rebuildListType) and worker/mutation.go
 * (buildIndexes, undoSchemaUpdate). Changed for ultragraphdb: translated to Rust; the indexes are built in memory,
 * Dgraph's temporary Badger, and one batch swaps them in: for every key under a rebuilt prefix the built complete
 * list, or a deletion where the build produced none, at the change's timestamp; for every key under a removed prefix
 * a deletion at the batch's commit timestamp; the list-type rewrite and a `@count` predicate's data lists; and the
 * schema record. A failed build deletes, in one batch, every key under a rebuilt prefix the previous schema does
 * not declare, and restores the previous schema, whose indexes writes kept complete (ledger 39). A vector index is
 * built in place, in one request reading each key's merged list, its predicates emptied by the same request, while
 * the change holds every commit back. A list-type rewrite keeps a list's language values; Dgraph's, without
 * `@count`, writes the untagged value alone. A count rebuild lists no subject whose list is empty, where Dgraph's
 * lists it under count 0.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;

use ultragraphdb_core_keys::{STAR, count_key, index_key, parse, reverse_key};
use ultragraphdb_core_schema::schema_entry;
use ultragraphdb_core_types::{TypeId, Value, convert, marshal};
use ultragraphdb_proto_pb::pb::schema_update::Directive;
use ultragraphdb_proto_pb::pb::{DirectedEdge, Posting, SchemaUpdate, directed_edge::Op};
use ultragraphdb_store_posting::{CachedList, List, PostingError, Request};
use ultragraphdb_traits_store::{Entry, Executor, ListCache, Scan, Store, StoreSnapshot};

use crate::errors::apply_error::ApplyError;
use crate::types::applier::Applier;
use crate::types::index_op::IndexOp;
use crate::types::index_rebuild::IndexRebuild;
use crate::types::index_schema::IndexSchema;
use crate::utils::index::{Maintain, index_vector};
use crate::utils::lock::lock;
use crate::utils::prefixes::{
    attr_data_prefix, attr_reverse_prefix, count_prefixes, reverse_count_prefixes,
    reverse_prefixes, tokenizer_prefixes, vector_edge_prefixes,
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

/// What a build produced: the lists it built, by key, each holding its postings in its scratch layer at the change's
/// timestamp; the data and reverse lists it rewrites whole; and a vector index's request.
struct Built<'a, S, C> {
    index: BTreeMap<Vec<u8>, List>,
    rewrite: BTreeMap<Vec<u8>, List>,
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
        self.build_and_swap(id, rebuild)
    }

    /// A build run on the executor: built outside the apply lock, swapped under it.
    pub(crate) fn run_job(&self, id: u64, rebuild: IndexRebuild) {
        let built = self.build(&rebuild);
        let _apply = lock(&self.apply);
        let outcome = match built {
            Ok(built) => self.swap(id, &rebuild, built),
            Err(error) => self.cleanup(id, &rebuild, &error).and(Err(error)),
        };
        if let Err(error) = outcome {
            tracing::error!(attr = rebuild.attr(), %error, "error in building indexes, aborting");
        }
    }

    /// Builds and swaps under the apply lock the caller holds.
    pub(crate) fn build_and_swap(
        &self,
        id: u64,
        rebuild: &IndexRebuild,
    ) -> Result<u64, ApplyError> {
        match self.build(rebuild) {
            Ok(built) => self.swap(id, rebuild, built),
            Err(error) => {
                tracing::error!(attr = rebuild.attr(), %error, "error in building indexes, aborting");
                self.cleanup(id, rebuild, &error)?;
                Err(error)
            }
        }
    }

    /// The build: every list of the change's indexes from the data at its timestamp.
    fn build(&self, rebuild: &IndexRebuild) -> Result<Built<'_, S, C>, ApplyError> {
        let attr = rebuild.attr();
        let new = rebuild.current_schema();
        let start_ts = rebuild.start_ts();
        let plan = plan(rebuild)?;
        self.lists.gate(start_ts)?;
        let snapshot = self.lists.store().snapshot(start_ts)?;
        let mut built = Built {
            index: BTreeMap::new(),
            rewrite: BTreeMap::new(),
            vector: None,
            plan,
        };
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
        if scans_data {
            for (key, list) in lists_under(&snapshot, &attr_data_prefix(attr)?)? {
                let uid = parse(&key)?.uid();
                let postings = postings(&list, start_ts)?;
                if !built.plan.tokenizers.is_empty() {
                    for p in &postings {
                        let lang = String::from_utf8_lossy(&p.lang_tag);
                        for token in tokens_schema.tokens(&lang, &posting_val(p))? {
                            add(&mut built.index, index_key(attr, &token)?, start_ts, uid, p)?;
                        }
                    }
                }
                if built.plan.reverse {
                    for p in &postings {
                        add(
                            &mut built.index,
                            reverse_key(attr, p.uid)?,
                            start_ts,
                            uid,
                            p,
                        )?;
                    }
                }
                if built.plan.count {
                    let size = list.length(start_ts, 0);
                    if size > 0 {
                        let key = count_key(attr, size as u32, false)?;
                        add(&mut built.index, key, start_ts, uid, &Posting::default())?;
                    }
                }
                if built.plan.list_type || new.count {
                    let rewritten = if built.plan.list_type {
                        re_identified(list, start_ts, &postings, new)?
                    } else {
                        list
                    };
                    built.rewrite.insert(key, rewritten);
                }
            }
        }
        if (built.plan.count || built.plan.reverse_count)
            && new.directive == Directive::Reverse as i32
        {
            let reverse: Vec<(Vec<u8>, List)> = if built.plan.reverse {
                built
                    .index
                    .iter()
                    .filter(|(k, _)| parse(k).is_ok_and(|pk| pk.is_reverse()))
                    .map(|(k, l)| (k.clone(), l.clone()))
                    .collect()
            } else {
                lists_under(&snapshot, &attr_reverse_prefix(attr)?)?
            };
            for (key, list) in reverse {
                let target = parse(&key)?.uid();
                let size = list.length(start_ts, 0);
                if size > 0 {
                    let count = count_key(attr, size as u32, true)?;
                    add(
                        &mut built.index,
                        count,
                        start_ts,
                        target,
                        &Posting::default(),
                    )?;
                }
                if !built.plan.reverse && new.count {
                    built.rewrite.insert(key, list);
                }
            }
        }
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
    ) -> Result<u64, ApplyError> {
        let attr = rebuild.attr();
        if !self.still_rebuilding(attr, id) {
            tracing::info!(attr, "rebuild discarded: its predicate was dropped");
            self.lists.resume_rollups();
            return Ok(0);
        }
        let start_ts = rebuild.start_ts();
        let new = rebuild.current_schema().clone();
        let latest = self.lists.latest_ts();
        self.lists.gate(latest)?;
        let snapshot = self.lists.store().snapshot(latest)?;
        let max_list_size = self.lists.config().max_list_size();
        let Built {
            index,
            rewrite,
            vector,
            plan,
        } = built;
        let mut request = vector.unwrap_or_else(|| self.lists.request(latest));
        let mut written: BTreeSet<Vec<u8>> = BTreeSet::new();
        for (key, list) in index.into_iter().chain(rewrite) {
            for entry in complete_entries(list, &key, start_ts, max_list_size)? {
                written.insert(entry.key().to_vec());
                request.add_entry_at_version(entry);
            }
        }
        for prefix in &plan.rebuilt {
            for key in keys_under(&snapshot, prefix)? {
                if !written.contains(&key) {
                    request.add_entry_at_version(Entry::deletion(key, start_ts));
                }
            }
        }
        for prefix in &plan.removed {
            for key in keys_under(&snapshot, prefix)? {
                request.add_entry(Entry::deletion(key, 0));
            }
        }
        request.add_entry(schema_entry(&new, 0)?);
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
        self.fence(plan.removed, ts);
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
            return Ok(());
        }
        tracing::error!(attr, %error, "error in building indexes, aborting");
        // Dgraph's undoSchemaUpdate reloads the previous schema record, which the swap never replaced.
        if let Some(old) = rebuild.old_schema() {
            self.schema.set(attr, old.clone());
        }
        self.schema.delete_mut_schema(attr);
        let declared = declared_prefixes(attr, rebuild.old_schema())?;
        let undeclared: Vec<Vec<u8>> = plan(rebuild)?
            .rebuilt
            .into_iter()
            .filter(|p| !declared.contains(p))
            .collect();
        let outcome = self.delete_under(&undeclared);
        self.lists.resume_rollups();
        let ts = outcome?;
        if ts > 0 {
            self.fence(undeclared, ts);
        }
        Ok(())
    }

    /// Deletes every key under `prefixes`, in one batch at its commit timestamp: the timestamp, 0 for no key.
    fn delete_under(&self, prefixes: &[Vec<u8>]) -> Result<u64, ApplyError> {
        let latest = self.lists.latest_ts();
        self.lists.gate(latest)?;
        let snapshot = self.lists.store().snapshot(latest)?;
        let mut request = self.lists.request(latest);
        let mut any = false;
        for prefix in prefixes {
            for key in keys_under(&snapshot, prefix)? {
                request.add_entry(Entry::deletion(key, 0));
                any = true;
            }
        }
        if !any {
            return Ok(0);
        }
        request.advance_epoch_on_commit();
        let mut batch = self.lists.batch();
        batch.push(request);
        Ok(self.lists.commit(batch)?.into_iter().next().unwrap_or(0))
    }
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

/// The prefixes of the indexes `old` declares.
fn declared_prefixes(attr: &str, old: Option<&SchemaUpdate>) -> Result<Vec<Vec<u8>>, ApplyError> {
    let Some(old) = old else {
        return Ok(Vec::new());
    };
    let mut prefixes = Vec::new();
    if old.directive == Directive::Index as i32 {
        for t in &old.tokenizer {
            prefixes.extend(tokenizer_prefixes(attr, t)?);
        }
    }
    if old.directive == Directive::Reverse as i32 {
        prefixes.extend(reverse_prefixes(attr)?);
    }
    if old.count {
        prefixes.extend(count_prefixes(attr)?);
    }
    Ok(prefixes)
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

/// The keys under `prefix` at `snapshot`, split parts included.
fn keys_under<T: StoreSnapshot>(snapshot: &T, prefix: &[u8]) -> Result<Vec<Vec<u8>>, ApplyError> {
    snapshot
        .scan(&Scan::prefix(prefix.to_vec()))
        .map(|item| Ok(item?.key().to_vec()))
        .collect()
}

/// The lists under `prefix` at `snapshot`, each read with its parts, split parts left out as keys.
fn lists_under<T: StoreSnapshot>(
    snapshot: &T,
    prefix: &[u8],
) -> Result<Vec<(Vec<u8>, List)>, ApplyError> {
    let mut out = Vec::new();
    for key in keys_under(snapshot, prefix)? {
        if parse(&key)?.has_start_uid() {
            continue;
        }
        let (list, _) = List::read(&key, snapshot)?;
        out.push((key, list));
    }
    Ok(out)
}

/// Adds `uid` to the list of `key` the build holds, with the facets of `source`.
fn add(
    built: &mut BTreeMap<Vec<u8>, List>,
    key: Vec<u8>,
    start_ts: u64,
    uid: u64,
    source: &Posting,
) -> Result<(), ApplyError> {
    let list = built.entry(key.clone()).or_insert_with(|| {
        let mut l = List::empty(key);
        l.set_ts(start_ts);
        l
    });
    let edge = DirectedEdge {
        value_id: uid,
        op: Op::Set as i32,
        facets: source.facets.clone(),
        ..DirectedEdge::default()
    };
    list.add_mutation(&edge, None)?;
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
