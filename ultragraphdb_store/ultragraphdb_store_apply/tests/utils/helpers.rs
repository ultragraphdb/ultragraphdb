/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the unit tests and the scenarios share: an applier over a fault-free simulated store and a stepped
//! executor, edges, schema text applied, and reads of the lists the applier writes.

use std::ops::ControlFlow;
use std::sync::Arc;

use ultragraphdb_core_keys::{
    BYTE_INDEX, attr_in_root_namespace, count_key, data_key, index_key, parse, predicate_prefix,
    reverse_key,
};
use ultragraphdb_core_catalog::parse as parse_schema;
use ultragraphdb_core_tok::{build_tokens, get_tokenizer, get_tokenizer_for_lang};
use ultragraphdb_core_types::Value;
use ultragraphdb_proto_pb::pb::{DirectedEdge, Mutations, SchemaUpdate, directed_edge, posting};
use ultragraphdb_store_apply::{Applier, ApplyError};
use ultragraphdb_store_posting::{CachedList, List, Lists, ListsConfig, PostingError};
use ultragraphdb_store_sim::{
    FaultConfig, Seed, SimClock, SimListCache, SimStore, SteppedExecutor, Stream,
};
use ultragraphdb_traits_store::{Scan, Store, StoreSnapshot};

pub type TestApplier = Applier<SimStore, SimListCache<CachedList>, SteppedExecutor>;

/// A fault-free simulated store.
pub fn new_store() -> SimStore {
    SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new())
}

/// An applier over a fault-free simulated store, its rebuilds on a stepped executor.
pub fn new_applier() -> Arc<TestApplier> {
    applier_over(new_store())
}

/// An applier over `store`, its schema loaded from it.
pub fn applier_over(store: SimStore) -> Arc<TestApplier> {
    let seed = Seed::new(1);
    let cache = SimListCache::new(1024, seed.stream(Stream::Cache));
    let lists = Lists::new(store, cache, ListsConfig::default());
    let executor = SteppedExecutor::new(seed.stream(Stream::Executor));
    Arc::new(Applier::new(lists, executor, 7).expect("the applier opens"))
}

/// The namespaced attribute of `name` in the root namespace.
pub fn attr(name: &str) -> String {
    attr_in_root_namespace(name)
}

/// The predicate updates of schema text.
pub fn updates(text: &str) -> Vec<SchemaUpdate> {
    parse_schema(text)
        .expect("the schema parses")
        .into_parts()
        .0
}

/// Applies the schema text, as an alter does: the change's timestamp. Rebuilds wait on the executor.
pub fn alter(a: &Arc<TestApplier>, text: &str) -> Result<u64, ApplyError> {
    a.alter(updates(text))
}

/// Applies the schema text and runs every rebuild it started.
pub fn alter_now(a: &Arc<TestApplier>, text: &str) -> u64 {
    let ts = alter(a, text).expect("the schema change");
    a.executor().run_until_idle();
    ts
}

/// Applies `edges` as one proposal reading at the newest timestamp: its commit timestamp.
pub fn mutate(a: &Arc<TestApplier>, edges: Vec<DirectedEdge>) -> Result<u64, ApplyError> {
    let m = Mutations {
        start_ts: a.lists().latest_ts(),
        edges,
        ..Mutations::default()
    };
    a.apply_mutations(&m)
}

/// An edge of `attr` from `entity`, untyped, as the chunker makes one.
pub fn set_str(name: &str, entity: u64, value: &str) -> DirectedEdge {
    DirectedEdge {
        entity,
        attr: attr(name),
        value: value.as_bytes().to_vec(),
        op: directed_edge::Op::Set as i32,
        ..DirectedEdge::default()
    }
}

/// [`set_str`] with a language.
pub fn set_lang(name: &str, entity: u64, value: &str, lang: &str) -> DirectedEdge {
    DirectedEdge {
        lang: lang.to_string(),
        ..set_str(name, entity, value)
    }
}

/// A delete of the value of [`set_str`].
pub fn del_str(name: &str, entity: u64, value: &str) -> DirectedEdge {
    DirectedEdge {
        op: directed_edge::Op::Del as i32,
        ..set_str(name, entity, value)
    }
}

/// An edge of `attr` from `entity` to `target`.
pub fn set_uid(name: &str, entity: u64, target: u64) -> DirectedEdge {
    DirectedEdge {
        entity,
        attr: attr(name),
        value_id: target,
        value_type: posting::ValType::Uid as i32,
        op: directed_edge::Op::Set as i32,
        ..DirectedEdge::default()
    }
}

/// A delete of the edge of [`set_uid`].
pub fn del_uid(name: &str, entity: u64, target: u64) -> DirectedEdge {
    DirectedEdge {
        op: directed_edge::Op::Del as i32,
        ..set_uid(name, entity, target)
    }
}

/// `<entity> <attr> * .`
pub fn star(name: &str, entity: u64) -> DirectedEdge {
    DirectedEdge {
        entity,
        attr: attr(name),
        value: b"_STAR_ALL".to_vec(),
        op: directed_edge::Op::Del as i32,
        ..DirectedEdge::default()
    }
}

/// The uids of `list` at `read_ts`.
pub fn list_uids(list: &List, read_ts: u64) -> Vec<u64> {
    let mut out = Vec::new();
    list.iterate(read_ts, 0, |p| {
        out.push(p.uid);
        Ok::<_, PostingError>(ControlFlow::Continue(()))
    })
    .expect("iterate");
    out
}

/// The uids of the list of `key` at `read_ts`.
pub fn uids_at(a: &TestApplier, key: &[u8], read_ts: u64) -> Vec<u64> {
    list_uids(&a.lists().read(key, read_ts).expect("read"), read_ts)
}

/// The uids of the list of `key` at the newest timestamp.
pub fn uids(a: &TestApplier, key: &[u8]) -> Vec<u64> {
    uids_at(a, key, a.lists().latest_ts())
}

/// The data key of `name` for `uid`.
pub fn dkey(name: &str, uid: u64) -> Vec<u8> {
    data_key(&attr(name), uid).expect("a data key")
}

/// The reverse key of `name` for `uid`.
pub fn rkey(name: &str, uid: u64) -> Vec<u8> {
    reverse_key(&attr(name), uid).expect("a reverse key")
}

/// The count key of `name` for `count`, the reverse one with `reverse`.
pub fn ckey(name: &str, count: u32, reverse: bool) -> Vec<u8> {
    count_key(&attr(name), count, reverse).expect("a count key")
}

/// The index key of `name` for each token `tokenizer` makes of `value` in language `lang`.
pub fn ikeys(name: &str, tokenizer: &str, value: &str, lang: &str) -> Vec<Vec<u8>> {
    let t = get_tokenizer(tokenizer).expect("a tokenizer");
    let t = get_tokenizer_for_lang(&t, lang);
    build_tokens(&Value::str(value), &t)
        .expect("tokens")
        .into_iter()
        .map(|token| index_key(&attr(name), &token).expect("an index key"))
        .collect()
}

/// The index key of `name` for the one token `tokenizer` makes of `value`.
pub fn ikey(name: &str, tokenizer: &str, value: &str) -> Vec<u8> {
    let keys = ikeys(name, tokenizer, value, "");
    assert_eq!(keys.len(), 1, "one token of {value}");
    keys.into_iter().next().unwrap_or_default()
}

/// The prefix of every index key of `name`.
pub fn index_prefix(name: &str) -> Vec<u8> {
    let mut p = predicate_prefix(&attr(name)).expect("a prefix");
    p.push(BYTE_INDEX);
    p
}

/// The keys under `prefix` whose list holds a posting at `read_ts`, split parts left out.
pub fn live_keys_at(a: &TestApplier, prefix: &[u8], read_ts: u64) -> Vec<Vec<u8>> {
    let snapshot = a.lists().store().snapshot(read_ts).expect("a snapshot");
    let keys: Vec<Vec<u8>> = snapshot
        .scan(&Scan::prefix(prefix.to_vec()))
        .map(|item| item.expect("scan").key().to_vec())
        .filter(|k| !parse(k).expect("a key").has_start_uid())
        .collect();
    keys.into_iter()
        .filter(|k| !uids_at(a, k, read_ts).is_empty())
        .collect()
}

/// [`live_keys_at`] at the newest timestamp.
pub fn live_keys(a: &TestApplier, prefix: &[u8]) -> Vec<Vec<u8>> {
    live_keys_at(a, prefix, a.lists().latest_ts())
}

/// Every stored key under `prefix`, any version, split parts included.
pub fn stored_keys(a: &TestApplier, prefix: &[u8]) -> Vec<Vec<u8>> {
    a.lists()
        .store()
        .contents()
        .keys()
        .keys()
        .filter(|k| k.starts_with(prefix))
        .cloned()
        .collect()
}

/// Commits `edges` to `key` straight through the posting lists, without index maintenance, at `commit_ts`: what
/// Go's index tests write by hand.
pub fn write_raw(
    a: &TestApplier,
    key: &[u8],
    edges: &[DirectedEdge],
    schema: Option<&SchemaUpdate>,
    commit_ts: u64,
) {
    next_commit_at(a, commit_ts);
    let mut batch = a.lists().batch();
    let mut request = batch.request(commit_ts - 1);
    let list = request.get(key).expect("read");
    edges
        .iter()
        .for_each(|e| list.add_mutation(e, schema).expect("mutation"));
    batch.push(request);
    assert_eq!(a.lists().commit(batch).expect("commit"), vec![commit_ts]);
}

/// Moves the applier's counter so that its next commit is at `commit_ts`.
pub fn next_commit_at(a: &TestApplier, commit_ts: u64) {
    a.lists().gate(commit_ts - 1).expect("gate");
    assert_eq!(
        a.lists().peek_ts(),
        commit_ts,
        "the counter is past {commit_ts}"
    );
}
