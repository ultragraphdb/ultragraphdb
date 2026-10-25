/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The commit of a batch of requests: under the writer lock, before commit timestamps are assigned, each request is
//! refused when it read below a drop or schema change it writes under, or read what a refused request wrote, and the
//! predicates it creates are written, or, when another commit defined them first, its values are converted to that
//! definition. Dgraph creates a predicate at its proposal's start timestamp and aborts its pending transactions on a
//! drop; the port has no pending transactions, so the request fails instead.

use std::collections::{BTreeMap, BTreeSet};

use ultragraphdb_core_keys::{parse, schema_key};
use ultragraphdb_core_schema::{SchemaMode, schema_entry};
use ultragraphdb_proto_pb::pb::{DirectedEdge, PostingList, SchemaUpdate, directed_edge, posting};
use ultragraphdb_store_posting::{
    Batch, CachedList, DEL, List, OVR, Request, fingerprint_edge, has_delete_all,
};
use ultragraphdb_traits_store::{Executor, ListCache, Store};

use crate::errors::apply_error::ApplyError;
use crate::types::applier::Applier;
use crate::types::applier::state::Fence;
use crate::utils::lock::lock;
use crate::utils::validate::validate_and_convert;

/// The predicates a request creates, by its position in its batch.
type Created = Vec<BTreeMap<String, SchemaUpdate>>;

impl<S, C, E> Applier<S, C, E>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    E: Executor + Send + Sync + 'static,
{
    /// Commits `batch`, whose requests create the predicates of `created`: see [`crate::ApplyBatch::commit`].
    pub(crate) fn commit_batch(
        &self,
        batch: Batch<'_, S, C>,
        created: Created,
    ) -> Result<Vec<Result<u64, ApplyError>>, ApplyError> {
        let _apply = lock(&self.apply);
        let fences = lock(&self.state).fences.clone();
        let mut refused: BTreeSet<Vec<u8>> = BTreeSet::new();
        let mut defined: BTreeMap<String, SchemaUpdate> = BTreeMap::new();
        let mut firsts: Vec<Vec<SchemaUpdate>> = vec![Vec::new(); created.len()];
        let mut index = 0usize;
        let results = self.lists.commit_with(batch, |request| {
            let at = index;
            index += 1;
            let mine = created.get(at).cloned().unwrap_or_default();
            let written: Vec<Vec<u8>> = request.written_keys().map(<[u8]>::to_vec).collect();
            let outcome =
                self.check_at_commit(request, &mine, &written, &fences, &refused, &defined);
            match &outcome {
                Ok(first) => {
                    for su in first {
                        defined.insert(su.predicate.clone(), su.clone());
                    }
                    if let Some(slot) = firsts.get_mut(at) {
                        *slot = first.clone();
                    }
                }
                Err(_) => refused.extend(written),
            }
            outcome.map(|_| ())
        })?;
        for (result, first) in results.iter().zip(&firsts) {
            if result.is_ok() {
                for su in first {
                    self.schema.set(&su.predicate, su.clone());
                }
            }
        }
        Ok(results)
    }

    /// The checks of one request at commit; the predicates it defines first.
    fn check_at_commit(
        &self,
        request: &mut Request<'_, S, C>,
        created: &BTreeMap<String, SchemaUpdate>,
        written: &[Vec<u8>],
        fences: &[Fence],
        refused: &BTreeSet<Vec<u8>>,
        defined: &BTreeMap<String, SchemaUpdate>,
    ) -> Result<Vec<SchemaUpdate>, ApplyError> {
        if request.read_keys().any(|k| refused.contains(k)) {
            return Err(ApplyError::Aborted());
        }
        let read_ts = request.read_ts();
        let schema_keys = created
            .keys()
            .map(|attr| schema_key(attr))
            .collect::<Result<Vec<_>, _>>()?;
        let fenced = fences.iter().any(|f| {
            written
                .iter()
                .chain(&schema_keys)
                .any(|k| f.refuses(read_ts, k))
        });
        if fenced {
            return Err(ApplyError::Aborted());
        }
        let mut first = Vec::new();
        for (attr, su) in created {
            let committed = defined
                .get(attr)
                .cloned()
                .or_else(|| self.schema.get(SchemaMode::Query, attr));
            match committed {
                Some(c) => convert_values(request, attr, &c)?,
                None => {
                    request.add_entry(schema_entry(su, 0)?);
                    first.push(su.clone());
                }
            }
        }
        Ok(first)
    }
}

/// Converts the values `request` writes to `attr`'s data lists to the schema `committed`, the posting identities that
/// depend on its list flag included, as Dgraph's apply loop converts a later proposal's edges to the schema the first
/// one created.
fn convert_values<S, C>(
    request: &mut Request<'_, S, C>,
    attr: &str,
    committed: &SchemaUpdate,
) -> Result<(), ApplyError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    let keys: Vec<Vec<u8>> = request
        .written_keys()
        .filter(|k| parse(k).is_ok_and(|pk| pk.attr() == attr && pk.is_data()))
        .map(<[u8]>::to_vec)
        .collect();
    let read_ts = request.read_ts();
    for key in keys {
        let entity = parse(&key)?.uid();
        let list = request.get(&key)?;
        let Some(delta) = list.delta().cloned() else {
            continue;
        };
        let mut postings = Vec::with_capacity(delta.postings.len());
        for p in delta.postings {
            if has_delete_all(&p) {
                postings.push(p);
                continue;
            }
            let is_ref = p.posting_type == posting::PostingType::Ref as i32;
            let mut edge = DirectedEdge {
                entity,
                attr: attr.to_string(),
                value: p.value.clone(),
                value_type: p.val_type,
                value_id: if is_ref { p.uid } else { 0 },
                lang: String::from_utf8_lossy(&p.lang_tag).into_owned(),
                op: edge_op(p.op) as i32,
                ..DirectedEdge::default()
            };
            validate_and_convert(&mut edge, committed)?;
            let mut q = p;
            q.value = edge.value.clone();
            q.val_type = edge.value_type;
            if !is_ref {
                q.uid = fingerprint_edge(&edge, committed.list);
            }
            postings.push(q);
        }
        // One posting per uid, the later winning, in uid order.
        postings.reverse();
        let mut seen = BTreeSet::new();
        postings.retain(|p| seen.insert(p.uid));
        postings.sort_by_key(|p| p.uid);
        let converted = PostingList { postings, ..delta };
        let taken = std::mem::replace(list, List::empty(key.clone()));
        *list = taken.with_scratch(read_ts, Some(converted));
    }
    Ok(())
}

/// The edge operation a posting's operation came from.
fn edge_op(op: u32) -> directed_edge::Op {
    match op {
        DEL => directed_edge::Op::Del,
        OVR => directed_edge::Op::Ovr,
        _ => directed_edge::Op::Set,
    }
}
