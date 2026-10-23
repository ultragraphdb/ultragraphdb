/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/index.go (addIndexMutations, addIndexMutation, addReverseMutationHelper,
 * addReverseAndCountMutation, handleDeleteAll, addCountMutation, updateCount, addMutationHelper,
 * AddMutationWithIndex). Changed for ultragraphdb: translated to Rust; a request of the local commit is the
 * transaction, and every list read is read whole (a list written blind is completed before it is read); the schema
 * is the caller's `IndexSchema`; the vector index is called through the index traits. Deviations from Dgraph, each
 * a ledger entry: deleting a value keeps the subject under the tokens the subject's other values still produce (35);
 * a star delete tokenizes each value with its language (34) and removes the subject from the count key it is listed
 * under, one for a scalar (36); a delete by value marks the vector dead, the value read before the delete applies
 * (46); setting a vector over one leaves its uid live (47).
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::BTreeSet;
use std::ops::ControlFlow;

use ultragraphdb_core_keys::{STAR, count_key, data_key, index_key, parse, reverse_key};
use ultragraphdb_core_tok::VEC_DEAD;
use ultragraphdb_core_types::{TypeId, Val, Value};
use ultragraphdb_proto_pb::pb::{DirectedEdge, Posting, directed_edge::Op};
use ultragraphdb_store_hnsw::{HnswError, HnswFactory, ViTxn, add_dead_node, bytes_as_float_array};
use ultragraphdb_store_posting::{
    CachedList, DEL, PostingError, Request, fingerprint_edge, new_posting,
};
use ultragraphdb_traits_index::{IndexFactory, VectorIndex};
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::apply_error::ApplyError;
use crate::types::index_schema::IndexSchema;
use crate::utils::counts::{CountParams, count_after_mutation, should_add_count_edge};

/// What index maintenance applies one edge with: the predicate's schema, whether an index is being built anywhere
/// (Dgraph's `IndexingInProgress`, which makes a count read iterate) and the draws of the vector index's inserts.
pub(crate) struct Maintain<'x, D> {
    pub schema: &'x IndexSchema,
    pub indexing: bool,
    pub draw: &'x mut D,
}

/// One value's index update: Dgraph's `indexMutationInfo`, with the tokens the subject's remaining values produce,
/// which a delete keeps.
struct IndexMutation<'e> {
    edge: &'e DirectedEdge,
    val: Val,
    op: Op,
    keep: BTreeSet<Vec<u8>>,
}

fn op_of(edge: &DirectedEdge) -> Op {
    Op::try_from(edge.op).unwrap_or(Op::Set)
}

/// A posting's value as stored, untyped: Dgraph's `valueToTypesVal`.
fn posting_val(p: &Posting) -> Val {
    Val::new(TypeId::new(p.val_type), Value::Bytes(p.value.clone()))
}

/// The postings of the list of `key` at the request's read timestamp.
fn postings_of<S, C>(
    request: &mut Request<'_, S, C>,
    key: &[u8],
) -> Result<Vec<Posting>, ApplyError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    let read_ts = request.read_ts();
    let mut out = Vec::new();
    request.get(key)?.iterate(read_ts, 0, |p| {
        out.push(p.clone());
        Ok::<_, PostingError>(ControlFlow::Continue(()))
    })?;
    Ok(out)
}

/// Dgraph's `AddMutationWithIndex`: applies `edge` to the data list of its subject with its index tokens, reverse
/// edge and count keys.
pub(crate) fn add_mutation_with_index<S, C, D>(
    request: &mut Request<'_, S, C>,
    m: &mut Maintain<'_, D>,
    edge: &DirectedEdge,
) -> Result<(), ApplyError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    D: FnMut() -> f64,
{
    if edge.attr.is_empty() {
        return Err(ApplyError::EmptyPredicate(
            edge.entity,
            edge.value_id,
            edge.value.clone(),
        ));
    }
    if op_of(edge) == Op::Del && edge.value == STAR.as_bytes() {
        return handle_delete_all(request, m, edge);
    }
    let mut edge = edge.clone();
    let do_update_index = m.schema.is_indexed();
    let has_count_index = m.schema.has_count();

    // The reverse edge first, whatever the forward edge does: a crash can come between them.
    if edge.value_id != 0 && m.schema.is_reversed() {
        add_reverse_and_count_mutation(request, m, &edge)?;
    }
    let (val, found, cp) =
        add_mutation_helper(request, m, do_update_index, has_count_index, &mut edge)?;
    if has_count_index && cp.after != cp.before {
        update_count(request, m, &cp)?;
    }
    if do_update_index {
        if found && let Some(val) = val {
            let keep = remaining_tokens(request, m, &edge)?;
            add_index_mutations(
                request,
                m,
                IndexMutation {
                    edge: &edge,
                    val,
                    op: Op::Del,
                    keep,
                },
            )?;
        }
        if op_of(&edge) != Op::Del {
            let val = Val::new(
                TypeId::new(edge.value_type),
                Value::Bytes(edge.value.clone()),
            );
            add_index_mutations(
                request,
                m,
                IndexMutation {
                    edge: &edge,
                    val,
                    op: Op::Set,
                    keep: BTreeSet::new(),
                },
            )?;
        }
    }
    Ok(())
}

/// The index tokens of every value the subject's list holds after its mutation: the tokens a delete of one value
/// keeps the subject under (ledger 35).
fn remaining_tokens<S, C, D>(
    request: &mut Request<'_, S, C>,
    m: &Maintain<'_, D>,
    edge: &DirectedEdge,
) -> Result<BTreeSet<Vec<u8>>, ApplyError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    if !m.schema.has_tokenizers() {
        return Ok(BTreeSet::new());
    }
    let key = data_key(&edge.attr, edge.entity)?;
    let mut keep = BTreeSet::new();
    for p in postings_of(request, &key)? {
        let lang = String::from_utf8_lossy(&p.lang_tag).into_owned();
        keep.extend(m.schema.tokens(&lang, &posting_val(&p))?);
    }
    Ok(keep)
}

/// Dgraph's `addMutationHelper`: applies `t` to the subject's data list, reading the posting it replaces when an
/// index or a count needs it, and returns the value it replaced, whether it held one, and the count's move.
fn add_mutation_helper<S, C, D>(
    request: &mut Request<'_, S, C>,
    m: &Maintain<'_, D>,
    do_update_index: bool,
    has_count_index: bool,
    t: &mut DirectedEdge,
) -> Result<(Option<Val>, bool, CountParams), ApplyError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    let key = data_key(&t.attr, t.entity)?;
    let read_ts = request.read_ts();
    let is_list = m.schema.is_list();
    let get_uid = if t.value_type == TypeId::UID.enum_value() {
        t.value_id
    } else {
        fingerprint_edge(t, is_list)
    };
    let is_scalar_predicate = !is_list;
    let del_non_list_predicate =
        is_scalar_predicate && op_of(t) == Op::Del && t.value != STAR.as_bytes();

    let mut count_before = 0i64;
    let mut curr_post: Option<Posting> = None;
    if has_count_index {
        let list = request.get(&key)?;
        let (before, post) =
            list.get_posting_and_length_no_sort(read_ts, 0, get_uid, m.indexing)?;
        if before == -1 {
            return Err(ApplyError::TsTooOld("Add mutation count index"));
        }
        count_before = before;
        curr_post = post;
    } else if do_update_index || del_non_list_predicate {
        curr_post = request
            .get(&key)?
            .find_posting(read_ts, fingerprint_edge(t, is_list))?;
    }
    let found = curr_post.is_some();

    // A delete of a scalar's value that is not the value it holds is ignored. Scalar values share one fingerprint,
    // so the posting found is compared by value.
    if del_non_list_predicate && let Some(curr) = &curr_post {
        let new_post = new_posting(t)?;
        if !(curr.value == new_post.value && curr.val_type == new_post.val_type) {
            return Ok((None, false, CountParams::default()));
        }
    }

    let mut op = op_of(t);
    let live = found && curr_post.as_ref().is_some_and(|p| p.op != DEL);
    if !(has_count_index && !should_add_count_edge(live, &mut op)) {
        t.op = op as i32;
        let schema = m.schema.posting_schema();
        request
            .get_from_delta(&key)
            .add_mutation(t, schema.as_ref())?;
    }
    let val = curr_post
        .as_ref()
        .filter(|_| found && do_update_index)
        .map(posting_val);
    if has_count_index {
        let should_count_one_uid = is_scalar_predicate && !parse(&key)?.is_reverse();
        let count_after = count_after_mutation(count_before, found, op_of(t), should_count_one_uid);
        return Ok((
            val,
            found,
            CountParams {
                attr: t.attr.clone(),
                before: count_before,
                after: count_after,
                entity: t.entity,
                reverse: false,
            },
        ));
    }
    Ok((val, found, CountParams::default()))
}

/// Dgraph's `addReverseAndCountMutation`: adds the reverse edge of `t` to its target's reverse list, with the
/// reverse count; a set of a predicate of one uid first removes the reverse edge of the uid it replaces.
fn add_reverse_and_count_mutation<S, C, D>(
    request: &mut Request<'_, S, C>,
    m: &Maintain<'_, D>,
    t: &DirectedEdge,
) -> Result<(), ApplyError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    let key = reverse_key(&t.attr, t.value_id)?;
    let has_count_index = m.schema.has_count();
    let is_single_uid_update = m
        .schema
        .schema()
        .is_some_and(|s| !s.list && s.value_type == TypeId::UID.enum_value())
        && op_of(t) != Op::Del
        && t.value_id != 0;
    if is_single_uid_update {
        let data = data_key(&t.attr, t.entity)?;
        for p in postings_of(request, &data)? {
            let del = DirectedEdge {
                entity: t.entity,
                value_id: p.uid,
                attr: t.attr.clone(),
                op: Op::Del as i32,
                ..DirectedEdge::default()
            };
            add_reverse_and_count_mutation(request, m, &del)?;
        }
    }
    let edge = DirectedEdge {
        entity: t.value_id,
        value_id: t.entity,
        attr: t.attr.clone(),
        op: t.op,
        facets: t.facets.clone(),
        ..DirectedEdge::default()
    };
    let cp = add_reverse_mutation_helper(request, m, &key, has_count_index, edge)?;
    if has_count_index && cp.after != cp.before {
        update_count(request, m, &cp)?;
    }
    Ok(())
}

/// Dgraph's `addReverseMutationHelper`: applies the reverse edge to the reverse list of `key`, and returns the
/// reverse count's move.
fn add_reverse_mutation_helper<S, C, D>(
    request: &mut Request<'_, S, C>,
    m: &Maintain<'_, D>,
    key: &[u8],
    has_count_index: bool,
    mut edge: DirectedEdge,
) -> Result<CountParams, ApplyError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    let read_ts = request.read_ts();
    let (mut count_before, mut found) = (0i64, false);
    if has_count_index {
        let list = request.get(key)?;
        let (before, post) =
            list.get_posting_and_length_no_sort(read_ts, 0, edge.value_id, m.indexing)?;
        if before < 0 {
            return Err(ApplyError::TsTooOld("Adding reverse mutation helper count"));
        }
        count_before = before;
        found = post.is_some();
    }
    let mut op = op_of(&edge);
    if !(has_count_index && !should_add_count_edge(found, &mut op)) {
        edge.op = op as i32;
        let schema = m.schema.posting_schema();
        request
            .get_from_delta(key)
            .add_mutation(&edge, schema.as_ref())?;
    }
    if has_count_index {
        let should_count_one_uid = !m.schema.is_list() && !parse(key)?.is_reverse();
        let count_after =
            count_after_mutation(count_before, found, op_of(&edge), should_count_one_uid);
        return Ok(CountParams {
            attr: edge.attr.clone(),
            before: count_before,
            after: count_after,
            entity: edge.entity,
            reverse: true,
        });
    }
    Ok(CountParams::default())
}

/// Dgraph's `handleDeleteAll`: removes every posting of the subject, with each one's reverse edge or index tokens,
/// and the subject from the count key it is listed under.
fn handle_delete_all<S, C, D>(
    request: &mut Request<'_, S, C>,
    m: &mut Maintain<'_, D>,
    edge: &DirectedEdge,
) -> Result<(), ApplyError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    D: FnMut() -> f64,
{
    let key = data_key(&edge.attr, edge.entity)?;
    let postings = postings_of(request, &key)?;
    let mut del_edge = DirectedEdge {
        attr: edge.attr.clone(),
        op: edge.op,
        entity: edge.entity,
        ..DirectedEdge::default()
    };
    for p in &postings {
        if m.schema.is_reversed() {
            del_edge.value_id = p.uid;
            add_reverse_and_count_mutation(request, m, &del_edge)?;
        } else if m.schema.is_indexed() {
            // Each value's tokens in its own language (ledger 34).
            let lang_edge = DirectedEdge {
                lang: String::from_utf8_lossy(&p.lang_tag).into_owned(),
                ..edge.clone()
            };
            add_index_mutations(
                request,
                m,
                IndexMutation {
                    edge: &lang_edge,
                    val: posting_val(p),
                    op: Op::Del,
                    keep: BTreeSet::new(),
                },
            )?;
        }
    }
    if m.schema.has_count() {
        // A scalar is listed under count 1 whatever its language values (ledger 36).
        let plen = postings.len() as i64;
        let before = if m.schema.is_list() {
            plen
        } else {
            plen.min(1)
        };
        update_count(
            request,
            m,
            &CountParams {
                attr: edge.attr.clone(),
                before,
                after: 0,
                entity: edge.entity,
                reverse: false,
            },
        )?;
    }
    let schema = m.schema.posting_schema();
    request.get(&key)?.add_mutation(edge, schema.as_ref())?;
    Ok(())
}

/// Dgraph's `updateCount`: moves the subject from the count key of its count before to that of its count after.
fn update_count<S, C, D>(
    request: &mut Request<'_, S, C>,
    m: &Maintain<'_, D>,
    cp: &CountParams,
) -> Result<(), ApplyError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    let mut edge = DirectedEdge {
        value_id: cp.entity,
        attr: cp.attr.clone(),
        op: Op::Del as i32,
        ..DirectedEdge::default()
    };
    let schema = m.schema.posting_schema();
    if cp.before > 0 {
        let key = count_key(&cp.attr, count_of(cp.before), cp.reverse)?;
        request
            .get_from_delta(&key)
            .add_mutation(&edge, schema.as_ref())?;
    }
    if cp.after > 0 {
        edge.op = Op::Set as i32;
        let key = count_key(&cp.attr, count_of(cp.after), cp.reverse)?;
        request
            .get_from_delta(&key)
            .add_mutation(&edge, schema.as_ref())?;
    }
    Ok(())
}

/// A count as a count key holds it, Go's `uint32` conversion.
fn count_of(n: i64) -> u32 {
    n as u32
}

/// Dgraph's `addIndexMutations`: the index update of one value: its tokens, less those `keep` holds, set or
/// deleted; for a vector predicate, the vector inserted into the index, or its uid added to the dead list.
fn add_index_mutations<S, C, D>(
    request: &mut Request<'_, S, C>,
    m: &mut Maintain<'_, D>,
    info: IndexMutation<'_>,
) -> Result<(), ApplyError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    D: FnMut() -> f64,
{
    let attr = &info.edge.attr;
    let uid = info.edge.entity;
    if uid == 0 {
        return Err(ApplyError::InvalidUid());
    }
    if !m.schema.specs().is_empty() {
        // The value deleted, read before the delete applied, marks its uid dead; a set over a vector re-inserts it
        // and leaves it live.
        if info.op == Op::Del && op_of(info.edge) == Op::Del && info.val.tid() == TypeId::VFLOAT {
            mark_dead(request, attr, uid)?;
        }
        if info.op != Op::Del {
            let key = data_key(attr, uid)?;
            let read_ts = request.read_ts();
            let data = request.get(&key)?.all_values(read_ts)?;
            if let Some(first) = data.first()
                && first.tid() == TypeId::VFLOAT
            {
                let bytes = match first.value() {
                    Some(Value::Bytes(b)) => b.clone(),
                    _ => Vec::new(),
                };
                return insert_vector(request, m, attr, uid, &bytes);
            }
        }
    }
    let lang = info.edge.lang.as_str();
    let tokens = m.schema.tokens(lang, &info.val)?;
    let edge = DirectedEdge {
        value_id: uid,
        attr: attr.clone(),
        op: info.op as i32,
        ..DirectedEdge::default()
    };
    let schema = m.schema.posting_schema();
    for token in tokens.iter().filter(|t| !info.keep.contains(*t)) {
        let key = index_key(attr, token)?;
        request
            .get_from_delta(&key)
            .add_mutation(&edge, schema.as_ref())?;
    }
    Ok(())
}

/// Adds `uid` to the dead list of the vector predicate `attr`.
fn mark_dead<S, C>(request: &mut Request<'_, S, C>, attr: &str, uid: u64) -> Result<(), ApplyError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    let dead_attr = format!("{attr}{VEC_DEAD}");
    let key = data_key(&dead_attr, 1)?;
    let read_ts = request.read_ts();
    let list = request.get(&key)?;
    let existing = match list.value(read_ts) {
        Ok(v) => match v.into_value() {
            Some(Value::Bytes(b)) => Some(b),
            _ => None,
        },
        Err(PostingError(ultragraphdb_store_posting::PostingErrorEnum::NoValue)) => None,
        Err(e) => return Err(e.into()),
    };
    let dead = add_dead_node(existing.as_deref(), uid)?;
    let edge = DirectedEdge {
        entity: 1,
        attr: dead_attr,
        value: dead,
        ..DirectedEdge::default()
    };
    list.add_mutation(&edge, None)?;
    Ok(())
}

/// Inserts the vector `bytes` of `uid` into the index of `attr`, through the index's traits: the first vector index
/// of the predicate, as Dgraph takes the first.
fn insert_vector<S, C, D>(
    request: &mut Request<'_, S, C>,
    m: &mut Maintain<'_, D>,
    attr: &str,
    uid: u64,
    bytes: &[u8],
) -> Result<(), ApplyError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    D: FnMut() -> f64,
{
    let Some(spec) = m.schema.specs().first() else {
        return Ok(());
    };
    let (opts, bits) = (spec.opts().clone(), spec.factory().float_bits());
    let key = data_key(attr, uid)?;
    let txn = ViTxn::new(request);
    if bits == 64 {
        let vec = bytes_as_float_array::<f64>(bytes).ok_or_else(|| {
            HnswError::corrupted(&key, format!("a vector of {} bytes", bytes.len()))
        })?;
        let index = HnswFactory::<f64>::new().create_or_replace(attr, &opts, bits)?;
        VectorIndex::insert(&index, &txn, uid, &vec, m.draw)?;
    } else {
        let vec = bytes_as_float_array::<f32>(bytes).ok_or_else(|| {
            HnswError::corrupted(&key, format!("a vector of {} bytes", bytes.len()))
        })?;
        let index = HnswFactory::<f32>::new().create_or_replace(attr, &opts, bits)?;
        VectorIndex::insert(&index, &txn, uid, &vec, m.draw)?;
    }
    Ok(())
}

/// The index update a rebuild makes for one vector, `edge` setting it: Dgraph's `addIndexMutations` with the set of
/// `processAddIndexMutation`.
pub(crate) fn index_vector<S, C, D>(
    request: &mut Request<'_, S, C>,
    m: &mut Maintain<'_, D>,
    edge: &DirectedEdge,
) -> Result<(), ApplyError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    D: FnMut() -> f64,
{
    let val = Val::new(
        TypeId::new(edge.value_type),
        Value::Bytes(edge.value.clone()),
    );
    add_index_mutations(
        request,
        m,
        IndexMutation {
            edge,
            val,
            op: Op::Set,
            keep: BTreeSet::new(),
        },
    )
}
