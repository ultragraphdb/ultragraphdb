/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scenarios of schema changes and drops, the local commit with index entries and the vector index's maintenance,
//! run over an applier on the simulated store; and, in `changeset_steps`, those of changesets, index maintenance,
//! posting lists, the catalog's write checks, schema storage's writes and statement IDs, run over a changeset applier.

use std::sync::{Arc, Mutex};

use cucumber::{World, then, when};
use ultragraphdb_common_utils::go_farm::fingerprint64;
use ultragraphdb_core_catalog::{SchemaMode, SchemaState, initial_schema, initial_types};
use ultragraphdb_core_keys::{
    BYTE_SPLIT, data_prefix, namespace_attr, predicate_prefix, schema_key, type_key,
};
use ultragraphdb_core_tok::{VEC_DEAD, VEC_ENTRY, VEC_KEYWORD};
use ultragraphdb_core_types::{TypeId, Value};
use ultragraphdb_proto_pb::pb::{
    DirectedEdge, Mutations, TypeUpdate, mutations::DropOp, schema_update::Directive,
};
use ultragraphdb_store_apply::{Applier, ApplyError};
use ultragraphdb_store_hnsw::{PersistentHnsw, QueryCache, ViTxn, parse_edges};
use ultragraphdb_store_posting::{Lists, ListsConfig};
use ultragraphdb_store_sim::{Fault, OpRecord, Seed, SimListCache, SteppedExecutor, Stream};
use ultragraphdb_traits_index::{Options, accept_all};
use ultragraphdb_traits_store::{Store, StoreSnapshot};

// The unit tests' helpers, shared by path.
mod utils {
    #[allow(dead_code)]
    #[path = "changesets.rs"]
    pub mod changesets;
    #[allow(dead_code)]
    #[path = "helpers.rs"]
    pub mod helpers;
    #[allow(dead_code)]
    #[path = "probe.rs"]
    pub mod probe;
}

mod changeset_steps;

use crate::utils::helpers::{
    TestApplier, alter, alter_now, applier_over, attr, ckey, del_str, del_uid, dkey, ikey,
    index_prefix, live_keys, mutate, new_applier, new_store, next_commit_at, rkey, set_str,
    set_uid, stored_keys, uids, uids_at, updates,
};
use crate::utils::probe::Probe;

#[derive(Debug, Default, World)]
struct Steps {
    applier: Option<Arc<TestApplier>>,
    result: Option<Result<u64, ApplyError>>,
    ts: Vec<u64>,
    keys: Vec<Vec<u8>>,
    flags: Vec<bool>,
    schemas: Vec<String>,
    changesets: Option<Arc<crate::utils::changesets::TestChangesets>>,
    copies: Vec<Arc<crate::utils::changesets::TestChangesets>>,
    results:
        Vec<Result<ultragraphdb_store_apply::Applied, ultragraphdb_store_apply::ChangesetError>>,
    names: Vec<String>,
    counts: Vec<Vec<u32>>,
    statement: Option<u64>,
}

fn applier(w: &Steps) -> &Arc<TestApplier> {
    w.applier.as_ref().expect("an applier")
}

fn fresh(w: &mut Steps, schema: &str) -> Arc<TestApplier> {
    let a = new_applier();
    if !schema.is_empty() {
        alter_now(&a, schema);
    }
    w.applier = Some(Arc::clone(&a));
    a
}

fn ok(a: &Arc<TestApplier>, edges: Vec<DirectedEdge>) -> u64 {
    mutate(a, edges).expect("the edges apply")
}

/// The untagged value of `name` for `uid` at the newest timestamp, as bytes.
fn value_of(a: &TestApplier, name: &str, uid: u64) -> Option<(TypeId, Vec<u8>)> {
    let ts = a.lists().latest_ts();
    let list = a.lists().read(&dkey(name, uid), ts).ok()?;
    let v = list.value(ts).ok()?;
    match v.value() {
        Some(Value::Bytes(b)) => Some((v.tid(), b.clone())),
        _ => None,
    }
}

fn holds(a: &TestApplier, key: &[u8], uid: u64) -> bool {
    uids(a, key).contains(&uid)
}

/// Every key under `prefix` whose list holds `uid`.
fn keys_holding(a: &TestApplier, prefix: &[u8], uid: u64) -> Vec<Vec<u8>> {
    live_keys(a, prefix)
        .into_iter()
        .filter(|k| holds(a, k, uid))
        .collect()
}

fn count_prefix(name: &str, reverse: bool) -> Vec<u8> {
    let mut k = ckey(name, 0, reverse);
    k.truncate(k.len() - 4);
    k
}

fn twin(prefix: &[u8]) -> Vec<u8> {
    let mut p = prefix.to_vec();
    p[0] = BYTE_SPLIT;
    p
}

/// The schema the store holds, loaded afresh.
fn stored_schema(a: &TestApplier) -> SchemaState {
    let state = SchemaState::new();
    state
        .load_from_db(a.lists().store())
        .expect("the schema loads");
    state
}

// The local commit, with index maintenance.

#[when("`name` of `0x1` holds no value and a request sets it to `A` and then deletes `A`")]
fn set_then_delete(w: &mut Steps) {
    let a = fresh(w, "name: string @index(exact) @count .");
    ok(&a, vec![set_str("name", 1, "A"), del_str("name", 1, "A")]);
}

#[then("after the commit, `name` of `0x1` holds no value")]
fn holds_no_value(w: &mut Steps) {
    let a = applier(w);
    assert!(value_of(a, "name", 1).is_none());
    assert!(keys_holding(a, &index_prefix("name"), 1).is_empty());
    assert!(keys_holding(a, &count_prefix("name", false), 1).is_empty());
}

#[when(
    "`friend` of `0x1` holds `0x2` in the store, and a request adds `0x3` without reading the list and then reads it"
)]
fn blind_then_read(w: &mut Steps) {
    // The reverse key of 0x5 is written blind by maintenance, after another edge put 0x9 in it.
    let a = fresh(w, "friend: [uid] @reverse .");
    ok(&a, vec![set_uid("friend", 1, 2), set_uid("friend", 9, 5)]);
    let batch = a.batch();
    let mut request = batch.request(a.lists().latest_ts());
    request.run_mutation(&set_uid("friend", 1, 3)).unwrap();
    request.run_mutation(&set_uid("friend", 1, 5)).unwrap();
    let read_ts = request.read_ts();
    let data = request.request().get(&dkey("friend", 1)).unwrap();
    w.ts = crate::utils::helpers::list_uids(data, read_ts);
    let rev = request.request().get(&rkey("friend", 5)).unwrap();
    w.keys = vec![
        crate::utils::helpers::list_uids(rev, read_ts)
            .into_iter()
            .flat_map(|u| u.to_be_bytes())
            .collect(),
    ];
}

#[then("the read holds `0x2` and `0x3`")]
fn read_whole(w: &mut Steps) {
    assert_eq!(w.ts, vec![2, 3, 5]);
    let reverse: Vec<u64> = w.keys[0]
        .chunks(8)
        .map(|c| u64::from_be_bytes(c.try_into().unwrap()))
        .collect();
    assert_eq!(
        reverse,
        vec![1, 9],
        "the reverse list written blind is read whole"
    );
}

#[when("a request writing three lists commits at 12 while another reader reads at 12 repeatedly")]
fn three_lists(w: &mut Steps) {
    let a = fresh(w, "name: string @index(exact) @count .");
    a.lists().store().take_log();
    next_commit_at(a.as_ref(), 12);
    w.keys = vec![
        dkey("name", 1),
        ikey("name", "exact", "big"),
        ckey("name", 1, false),
    ];
    let ts = ok(&a, vec![set_str("name", 1, "big")]);
    // A read below the commit sees none of it.
    w.flags = w
        .keys
        .iter()
        .map(|k| uids_at(&a, k, ts - 1).is_empty())
        .collect();
    w.ts = vec![ts];
}

#[then("every read sees the changes to all three lists or to none of them")]
fn visible_whole(w: &mut Steps) {
    let a = applier(w);
    assert_eq!(w.ts, vec![12]);
    assert_eq!(w.flags, vec![true, true, true], "none before the commit");
    assert!(
        w.keys.iter().all(|k| !uids_at(a, k, 12).is_empty()),
        "all after it"
    );
    let batches: Vec<_> = a
        .lists()
        .store()
        .take_log()
        .into_iter()
        .filter_map(|op| match op {
            OpRecord::Applied { batch, .. } => Some(batch),
            _ => None,
        })
        .collect();
    let holding: Vec<_> = batches
        .iter()
        .filter(|b| {
            b.entries()
                .iter()
                .any(|e| w.keys.contains(&e.key().to_vec()))
        })
        .collect();
    assert_eq!(holding.len(), 1, "one store batch holds the three lists");
    for k in &w.keys {
        let e = holding[0]
            .entries()
            .iter()
            .find(|e| e.key() == k.as_slice())
            .unwrap();
        assert_eq!(e.version(), 12);
    }
}

#[when("a request fails after building writes and before committing")]
fn fails_after_building(w: &mut Steps) {
    let a = fresh(w, "name: string @index(exact) @count .\nzz: int .");
    w.keys = vec![
        dkey("name", 1),
        ikey("name", "exact", "big"),
        ckey("name", 1, false),
    ];
    w.result = Some(mutate(
        &a,
        vec![set_str("name", 1, "big"), set_str("zz", 1, "not a number")],
    ));
}

#[then("the store holds none of its writes")]
fn none_written(w: &mut Steps) {
    assert!(w.result.as_ref().unwrap().is_err());
    let a = applier(w);
    for k in &w.keys {
        assert!(stored_keys(a, k).is_empty(), "{k:?}");
    }
}

// Vector index maintenance.

const EMB: &str =
    "emb: float32vector @index(hnsw(maxLevels:\"3\", efConstruction:\"8\", efSearch:\"6\")) .";

fn vectors(a: &Arc<TestApplier>) {
    let edges: Vec<DirectedEdge> = (1..=12u64)
        .map(|uid| set_str("emb", uid, &format!("[{}, {}]", uid, uid % 4)))
        .collect();
    for e in edges {
        ok(a, vec![e]);
    }
}

/// The uids of the dead list of `emb`.
fn dead(a: &TestApplier) -> Vec<u64> {
    let key = dkey(&format!("emb{VEC_DEAD}"), 1);
    let ts = a.lists().latest_ts();
    match a.lists().read(&key, ts).unwrap().value(ts) {
        Ok(v) => match v.value() {
            Some(Value::Bytes(b)) => parse_edges(&String::from_utf8_lossy(b)).unwrap(),
            _ => Vec::new(),
        },
        Err(_) => Vec::new(),
    }
}

#[when("a vector of `emb: float32vector @index(hnsw) .` is deleted by its value")]
fn delete_vector_by_value(w: &mut Steps) {
    let a = fresh(w, EMB);
    vectors(&a);
    ok(&a, vec![del_str("emb", 5, "[5, 1]")]);
}

#[then("its uid is added to the dead list")]
fn dead_listed(w: &mut Steps) {
    // Dgraph applies the delete first and finds no vector to mark (ledger 46).
    let a = applier(w);
    assert_eq!(dead(a), vec![5]);
    assert!(value_of(a, "emb", 5).is_none());
}

#[when("a vector of `emb: float32vector @index(hnsw) .` is set over the vector it holds")]
fn overwrite_vector(w: &mut Steps) {
    let a = fresh(w, EMB);
    vectors(&a);
    ok(&a, vec![set_str("emb", 5, "[50, 50]")]);
}

#[when("a vector of `emb: float32vector @index(hnsw) .` is deleted and then set again")]
fn delete_then_set_vector(w: &mut Steps) {
    let a = fresh(w, EMB);
    vectors(&a);
    ok(&a, vec![del_str("emb", 5, "[5, 1]")]);
    assert_eq!(dead(&a), vec![5]);
    ok(&a, vec![set_str("emb", 5, "[50, 50]")]);
}

#[then("its uid is not in the dead list, and the index still finds it")]
fn not_dead(w: &mut Steps) {
    // Dgraph marks the uid dead while setting it (ledger 47), and keeps a deleted uid dead when it is set again
    // (ledger 53).
    let a = applier(w);
    assert!(dead(a).is_empty());
    let mut o = Options::new();
    for (k, v) in [
        ("maxLevels", "3"),
        ("efConstruction", "8"),
        ("efSearch", "6"),
    ] {
        o.set_opt(
            k,
            ultragraphdb_traits_index::OptionValue::Int(v.parse().unwrap()),
        );
    }
    let index = PersistentHnsw::<f32>::new(&attr("emb"), &o, 32).unwrap();
    let mut request = a.lists().request(a.lists().latest_ts());
    let read_ts = request.read_ts();
    let txn = ViTxn::new(&mut request);
    let found = index
        .search(
            &QueryCache::new(&txn, read_ts),
            &[50.0, 50.0],
            1,
            accept_all::<f32>,
        )
        .unwrap();
    assert_eq!(found, vec![5]);
}

#[when("`emb: float32vector @index(hnsw(maxLevels:\"0\")) .` is applied")]
fn zero_levels(w: &mut Steps) {
    let a = fresh(w, "");
    w.result = Some(alter(
        &a,
        "emb: float32vector @index(hnsw(maxLevels:\"0\")) .",
    ));
}

#[then(
    "the change fails with `IndexFactory: hnsw issues this error: 'the vector index option maxLevels must be at least 1, got 0'`, and the schema does not hold `emb`"
)]
fn zero_refused(w: &mut Steps) {
    // Go accepts the option and panics at the first search.
    let err = w.result.take().unwrap().unwrap_err();
    assert_eq!(
        err.to_string(),
        "IndexFactory: hnsw issues this error: 'the vector index option maxLevels must be at least 1, got 0'"
    );
    let a = applier(w);
    assert!(a.schema().get(SchemaMode::Query, &attr("emb")).is_none());
    assert!(
        stored_schema(a)
            .get(SchemaMode::Query, &attr("emb"))
            .is_none()
    );
}

// Schema changes.

#[when("a predicate holds 1,000 values and `@index(exact)` is added")]
fn thousand(w: &mut Steps) {
    let a = fresh(w, "v: string .");
    let edges: Vec<DirectedEdge> = (1..=1000u64)
        .map(|uid| set_str("v", uid, &format!("value {uid}")))
        .collect();
    ok(&a, edges);
    alter(&a, "v: string @index(exact) .").unwrap();
    assert!(
        a.schema().indexing_in_progress(),
        "the index is built in the background"
    );
    a.executor().run_until_idle();
}

#[then("after the rebuild, the index holds a token for each of the 1,000 values")]
fn thousand_indexed(w: &mut Steps) {
    let a = applier(w);
    assert!(!a.schema().indexing_in_progress());
    let keys = live_keys(a, &index_prefix("v"));
    assert_eq!(keys.len(), 1000);
    for uid in [1u64, 500, 1000] {
        assert_eq!(
            uids(a, &ikey("v", "exact", &format!("value {uid}"))),
            vec![uid]
        );
    }
}

#[when("a predicate indexed by `term` changes to `exact` and the `exact` index is being built")]
fn interim(w: &mut Steps) {
    let a = fresh(w, "name: string @index(term) .");
    ok(&a, vec![set_str("name", 1, "big apple")]);
    alter(&a, "name: string @index(exact) .").unwrap();
    let q = a.schema().get(SchemaMode::Query, &attr("name")).unwrap();
    w.schemas = q.tokenizer.clone();
    w.flags = vec![q.directive == Directive::Index as i32];
    a.executor().run_until_idle();
}

#[then("the schema state used for queries reports the predicate with neither `term` nor `exact`")]
fn interim_empty(w: &mut Steps) {
    assert!(w.schemas.is_empty(), "{:?}", w.schemas);
    let q = applier(w)
        .schema()
        .get(SchemaMode::Query, &attr("name"))
        .unwrap();
    assert_eq!(q.tokenizer, ["exact"], "after the rebuild");
}

#[when("a value is written while its predicate's index is rebuilt")]
fn write_during(w: &mut Steps) {
    let a = fresh(w, "v: string .");
    ok(&a, vec![set_str("v", 1, "old")]);
    alter(&a, "v: string @index(exact) .").unwrap();
    ok(&a, vec![set_str("v", 2, "during")]);
    a.executor().run_until_idle();
}

#[then("after the rebuild, the index holds the value's tokens")]
fn during_indexed(w: &mut Steps) {
    let a = applier(w);
    assert_eq!(uids(a, &ikey("v", "exact", "during")), vec![2]);
    assert_eq!(uids(a, &ikey("v", "exact", "old")), vec![1]);
}

#[when(
    "`name: string @index(term) .` changes to `name: string @index(exact) .`, and `name` of `0x2` is set to `pear` while the `exact` index is built"
)]
fn term_to_exact(w: &mut Steps) {
    let a = fresh(w, "name: string @index(term) .");
    ok(&a, vec![set_str("name", 1, "big apple")]);
    alter(&a, "name: string @index(exact) .").unwrap();
    ok(&a, vec![set_str("name", 2, "pear")]);
    a.executor().run_until_idle();
}

#[then(
    "after the rebuild, no term key of `name` remains, and the exact key for `pear` contains `0x2`"
)]
fn no_term(w: &mut Steps) {
    let a = applier(w);
    let mut term = index_prefix("name");
    term.push(ultragraphdb_core_tok::IDENT_TERM);
    assert!(live_keys(a, &term).is_empty(), "{:?}", live_keys(a, &term));
    assert_eq!(uids(a, &ikey("name", "exact", "pear")), vec![2]);
    assert_eq!(uids(a, &ikey("name", "exact", "big apple")), vec![1]);
}

#[when(
    "`n: string @index(exact) .` holding `7` for `0x1` changes to `n: int @index(int) .`, and `n` of `0x1` is set to `8` while the index is built"
)]
fn string_to_int(w: &mut Steps) {
    let a = fresh(w, "n: string @index(exact) .");
    ok(&a, vec![set_str("n", 1, "7")]);
    alter(&a, "n: int @index(int) .").unwrap();
    ok(&a, vec![set_str("n", 1, "8")]);
    a.executor().run_until_idle();
}

#[then(
    "after the rebuild, no exact key of `n` remains, and the int keys of `n` list `0x1` under 8 and not under 7"
)]
fn int_keys(w: &mut Steps) {
    let a = applier(w);
    let mut exact = index_prefix("n");
    exact.push(ultragraphdb_core_tok::IDENT_EXACT);
    assert!(live_keys(a, &exact).is_empty());
    let int_key = |v: i64| {
        let t = ultragraphdb_core_tok::get_tokenizer("int").unwrap();
        let token = ultragraphdb_core_tok::build_tokens(&Value::Int(v), &t)
            .unwrap()
            .remove(0);
        ultragraphdb_core_keys::index_key(&attr("n"), &token).unwrap()
    };
    assert!(holds(a, &int_key(8), 1));
    assert!(!holds(a, &int_key(7), 1));
}

#[when("a predicate indexed by `term` and `exact` drops `term`")]
fn drop_term(w: &mut Steps) {
    let a = fresh(w, "name: string @index(term, exact) .");
    ok(&a, vec![set_str("name", 1, "big apple")]);
    alter_now(&a, "name: string @index(exact) .");
}

#[then("its term keys are gone and its exact keys remain")]
fn term_gone(w: &mut Steps) {
    let a = applier(w);
    let mut term = index_prefix("name");
    term.push(ultragraphdb_core_tok::IDENT_TERM);
    assert!(live_keys(a, &term).is_empty());
    assert_eq!(uids(a, &ikey("name", "exact", "big apple")), vec![1]);
}

#[when("`tag: string .` with values becomes `tag: [string] .`")]
fn scalar_to_list(w: &mut Steps) {
    let a = fresh(w, "tag: string @index(exact) .");
    ok(&a, vec![set_str("tag", 1, "one"), set_str("tag", 2, "two")]);
    alter_now(&a, "tag: [string] @index(exact) .");
}

#[then("every existing value is read back as an element of the list")]
fn read_as_list(w: &mut Steps) {
    let a = applier(w);
    for (uid, v) in [(1u64, "one"), (2, "two")] {
        let posting_uids = uids(a, &dkey("tag", uid));
        assert!(
            posting_uids.contains(&fingerprint64(v.as_bytes())),
            "{uid}: {posting_uids:?}"
        );
        assert!(!posting_uids.contains(&u64::MAX));
    }
    ok(a, vec![set_str("tag", 1, "uno")]);
    assert_eq!(
        uids(a, &dkey("tag", 1)).len(),
        2,
        "a second value is another element"
    );
    assert_eq!(
        uids(a, &ikey("tag", "exact", "one")),
        vec![1],
        "the index is kept"
    );
}

#[when("`tag: [string] .` holding values is changed to `tag: string .`")]
fn list_to_scalar(w: &mut Steps) {
    let a = fresh(w, "tag: [string] .");
    ok(&a, vec![set_str("tag", 1, "one")]);
    w.result = Some(alter(&a, "tag: string ."));
}

#[then(
    "the change fails with `Schema change not allowed from [string] => string without deleting pred: tag`"
)]
fn list_to_scalar_refused(w: &mut Steps) {
    assert_eq!(
        w.result.take().unwrap().unwrap_err().to_string(),
        "Schema change not allowed from [string] => string without deleting pred: tag"
    );
}

#[when("`tag: [string] @index(exact) .` holding no values is changed to `tag: string .`")]
fn empty_list_to_scalar(w: &mut Steps) {
    let a = fresh(w, "tag: [string] @index(exact) .");
    w.result = Some(alter(&a, "tag: string ."));
}

#[then(
    "the change fails with `Type can't be changed from list to scalar for attr: [tag] without dropping it first.`, and the schema still declares `tag: [string] @index(exact) .`"
)]
fn empty_list_refused(w: &mut Steps) {
    assert_eq!(
        w.result.take().unwrap().unwrap_err().to_string(),
        "Type can't be changed from list to scalar for attr: [tag] without dropping it first."
    );
    let a = applier(w);
    let expected = updates("tag: [string] @index(exact) .").remove(0);
    for state in [a.schema(), &stored_schema(a)] {
        assert_eq!(
            state.get(SchemaMode::Query, &attr("tag")),
            Some(expected.clone())
        );
    }
    assert!(!a.schema().indexing_in_progress());
}

#[when("`friend: uid .` holds edges and is changed to `friend: string .`")]
fn uid_to_string(w: &mut Steps) {
    let a = fresh(w, "friend: uid .");
    ok(&a, vec![set_uid("friend", 1, 2)]);
    w.result = Some(alter(&a, "friend: string ."));
}

#[then(
    "the change fails with `Schema change not allowed from scalar to uid or vice versa while there is data for pred: friend`"
)]
fn uid_to_string_refused(w: &mut Steps) {
    assert_eq!(
        w.result.take().unwrap().unwrap_err().to_string(),
        "Schema change not allowed from scalar to uid or vice versa while there is data for pred: friend"
    );
}

#[when("`token: uid .` holds no edges and is changed to `token: password .`")]
fn uid_to_password(w: &mut Steps) {
    let a = fresh(w, "token: uid .");
    w.result = Some(alter(&a, "token: password ."));
}

#[then("the change is applied")]
fn applied(w: &mut Steps) {
    assert!(w.result.take().unwrap().is_ok());
    let a = applier(w);
    a.executor().run_until_idle();
    assert!(!a.schema().indexing_in_progress());
}

#[when(
    "`friend: uid .` gained the edge `0x1 → 0x2` at 3 and lost it at 4, and is changed to `friend: string .` at 5"
)]
fn deltas_sum_to_nothing(w: &mut Steps) {
    let a = fresh(w, "");
    a.schema().parse_text("friend: uid .").unwrap();
    crate::utils::helpers::write_raw(&a, &dkey("friend", 1), &[set_uid("friend", 1, 2)], None, 3);
    crate::utils::helpers::write_raw(&a, &dkey("friend", 1), &[del_uid("friend", 1, 2)], None, 4);
    next_commit_at(&a, 5);
    w.result = Some(alter(&a, "friend: string ."));
}

#[when("`secret: string .` is changed to `secret: password .`")]
fn string_to_password(w: &mut Steps) {
    let a = fresh(w, "secret: string .");
    w.result = Some(alter(&a, "secret: password ."));
}

#[then("the change fails with `Schema change not allowed from STRING to PASSWORD`")]
fn password_refused(w: &mut Steps) {
    assert_eq!(
        w.result.take().unwrap().unwrap_err().to_string(),
        "Schema change not allowed from STRING to PASSWORD"
    );
}

#[when("`term` is replaced by `exact` and the `exact` build fails")]
fn failed_build(w: &mut Steps) {
    let a = fresh(w, "name: string @index(term) .");
    ok(
        &a,
        vec![
            set_str("name", 1, "big apple"),
            set_str("name", 2, "red pear"),
        ],
    );
    alter(&a, "name: string @index(exact) .").unwrap();
    // A write during the build maintains both indexes.
    ok(&a, vec![set_str("name", 3, "green fig")]);
    a.lists().store().inject(Fault::FailNextScanAfter(1));
    a.executor().run_until_idle();
}

#[then("the schema declares `term` and every term key is present")]
fn old_index_kept(w: &mut Steps) {
    // Dgraph restores the schema record over indexes already dropped (ledger 39).
    let a = applier(w);
    let expected = updates("name: string @index(term) .").remove(0);
    for state in [a.schema(), &stored_schema(a)] {
        assert_eq!(
            state.get(SchemaMode::Query, &attr("name")),
            Some(expected.clone())
        );
    }
    assert!(!a.schema().indexing_in_progress());
    for (uid, value) in [(1u64, "big apple"), (2, "red pear"), (3, "green fig")] {
        for word in value.split(' ') {
            assert!(holds(a, &ikey("name", "term", word), uid), "{word}");
        }
    }
    let mut exact = index_prefix("name");
    exact.push(ultragraphdb_core_tok::IDENT_EXACT);
    assert!(
        live_keys(a, &exact).is_empty(),
        "the exact keys written during the build are deleted"
    );
}

/// An applier whose lists split at a small size, so a list of a few hundred uids is split by its rollup.
fn splitting_applier(w: &mut Steps) -> Arc<TestApplier> {
    let seed = Seed::new(1);
    let cache = SimListCache::new(1024, seed.stream(Stream::Cache));
    let lists = Lists::new(
        new_store(),
        cache,
        ListsConfig::new(ultragraphdb_traits_store::PersistMode::Buffer, 64),
    );
    let executor = SteppedExecutor::new(seed.stream(Stream::Executor));
    let a = Arc::new(Applier::new(lists, executor, 7).unwrap());
    w.applier = Some(Arc::clone(&a));
    a
}

/// Writes 600 uids to `friend` of 0x1 and rolls the list up into parts.
fn split_friend(a: &Arc<TestApplier>) {
    alter_now(a, "friend: [uid] @reverse @count .");
    let edges: Vec<DirectedEdge> = (2..602u64).map(|t| set_uid("friend", 1, t)).collect();
    ok(a, edges);
    assert!(a.lists().rollup_key(&dkey("friend", 1)).unwrap());
    let parts = stored_keys(a, &twin(&predicate_prefix(&attr("friend")).unwrap()));
    assert!(parts.len() > 1, "the list is split: {} parts", parts.len());
}

fn drop_predicate(a: &Arc<TestApplier>, name: &str) -> Result<u64, ApplyError> {
    let m = Mutations {
        start_ts: a.lists().latest_ts(),
        edges: vec![DirectedEdge {
            attr: attr(name),
            value: b"_STAR_ALL".to_vec(),
            op: ultragraphdb_proto_pb::pb::directed_edge::Op::Del as i32,
            ..DirectedEdge::default()
        }],
        ..Mutations::default()
    };
    a.apply_mutations(&m)
}

fn drop_op(a: &Arc<TestApplier>, op: DropOp, value: &str) -> Result<u64, ApplyError> {
    let m = Mutations {
        start_ts: a.lists().latest_ts(),
        drop_op: op as i32,
        drop_value: value.to_string(),
        ..Mutations::default()
    };
    a.apply_mutations(&m)
}

#[when("a predicate whose lists are split is dropped")]
fn drop_split(w: &mut Steps) {
    let a = splitting_applier(w);
    split_friend(&a);
    drop_predicate(&a, "friend").unwrap();
}

#[then("no key of the predicate remains, split parts included")]
fn split_dropped(w: &mut Steps) {
    // Dgraph drops the 0x00 prefix alone and leaves the parts (ledger 7).
    let a = applier(w);
    let prefix = predicate_prefix(&attr("friend")).unwrap();
    assert!(stored_keys(a, &prefix).is_empty());
    assert!(stored_keys(a, &twin(&prefix)).is_empty());
    assert!(stored_keys(a, &schema_key(&attr("friend")).unwrap()).is_empty());
    assert!(a.schema().get(SchemaMode::Query, &attr("friend")).is_none());
}

#[when("`emb: float32vector @index(hnsw) .` holds vectors and is dropped")]
fn drop_vector(w: &mut Steps) {
    let a = fresh(w, EMB);
    vectors(&a);
    ok(&a, vec![del_str("emb", 3, "[3, 3]")]);
    for p in [
        format!("emb{VEC_ENTRY}"),
        format!("emb{VEC_KEYWORD}"),
        format!("emb{VEC_DEAD}"),
    ] {
        assert!(
            !stored_keys(&a, &predicate_prefix(&attr(&p)).unwrap()).is_empty(),
            "{p}"
        );
    }
    drop_predicate(&a, "emb").unwrap();
}

#[then("no key of `emb`, `emb__vector_entry`, `emb__vector_` or `emb__vector_dead` remains")]
fn vector_dropped(w: &mut Steps) {
    let a = applier(w);
    for p in [
        "emb".to_string(),
        format!("emb{VEC_ENTRY}"),
        format!("emb{VEC_KEYWORD}"),
        format!("emb{VEC_DEAD}"),
    ] {
        let prefix = predicate_prefix(&attr(&p)).unwrap();
        assert!(stored_keys(a, &prefix).is_empty(), "{p}");
        assert!(stored_keys(a, &twin(&prefix)).is_empty(), "{p}");
        assert!(
            stored_keys(a, &schema_key(&attr(&p)).unwrap()).is_empty(),
            "{p}"
        );
    }
}

fn with_types(a: &Arc<TestApplier>, ns: u64) {
    let t = TypeUpdate {
        type_name: namespace_attr(ns, "Person"),
        fields: vec![ultragraphdb_proto_pb::pb::SchemaUpdate {
            predicate: namespace_attr(ns, "name"),
            ..Default::default()
        }],
    };
    a.update_types(vec![t]).unwrap();
}

#[when("a namespace's data is dropped")]
fn drop_data(w: &mut Steps) {
    let a = fresh(w, "name: string @index(exact) .");
    with_types(&a, 0);
    ok(&a, vec![set_str("name", 1, "A")]);
    w.schemas = vec![format!(
        "{:?}",
        a.schema().get(SchemaMode::Query, &attr("name"))
    )];
    drop_op(&a, DropOp::Data, "0").unwrap();
}

#[then("its predicates hold no data and its schema and types are unchanged")]
fn data_dropped(w: &mut Steps) {
    let a = applier(w);
    assert!(stored_keys(a, &data_prefix(0)).is_empty());
    assert!(stored_keys(a, &twin(&data_prefix(0))).is_empty());
    for state in [a.schema(), &stored_schema(a)] {
        assert_eq!(
            vec![format!("{:?}", state.get(SchemaMode::Query, &attr("name")))],
            w.schemas
        );
        assert!(state.get_type(&namespace_attr(0, "Person")).is_some());
    }
}

/// Namespace 3's predicates and types, sorted, of a state.
fn ns_contents(state: &SchemaState, ns: u64) -> (Vec<String>, Vec<String>) {
    let in_ns = |s: &String| ultragraphdb_core_keys::parse_namespace(s).is_ok_and(|n| n == ns);
    let mut preds: Vec<String> = state.predicates().into_iter().filter(in_ns).collect();
    let mut types: Vec<String> = state.types().into_iter().filter(in_ns).collect();
    preds.sort();
    types.sort();
    (preds, types)
}

fn initial_contents(ns: u64) -> (Vec<String>, Vec<String>) {
    let mut preds: Vec<String> = initial_schema(ns, false)
        .into_iter()
        .map(|s| s.predicate)
        .collect();
    let mut types: Vec<String> = initial_types(ns, false)
        .into_iter()
        .map(|t| t.type_name)
        .collect();
    preds.sort();
    types.sort();
    (preds, types)
}

fn fill_namespace_three(a: &Arc<TestApplier>) {
    let mut ups = updates("name: string @index(exact) .");
    ups[0].predicate = namespace_attr(3, "name");
    a.alter(ups).unwrap();
    with_types(a, 3);
    ok(
        a,
        vec![DirectedEdge {
            attr: namespace_attr(3, "name"),
            ..set_str("name", 1, "A")
        }],
    );
}

#[when("everything in namespace 3 is dropped and the store is reopened")]
fn drop_ns_restart(w: &mut Steps) {
    let a = fresh(w, "name: string .");
    fill_namespace_three(&a);
    drop_op(&a, DropOp::AllInNs, "3").unwrap();
    let store = a.lists().store().clone();
    store.crash();
    store.recover();
    w.applier = Some(applier_over(store));
}

#[then("namespace 3 holds only its initial schema and types")]
fn ns_initial(w: &mut Steps) {
    // Dgraph deletes the namespace's schema and types in memory only, and they return at restart (ledger 8).
    let a = applier(w);
    assert_eq!(ns_contents(a.schema(), 3), initial_contents(3));
    assert!(stored_keys(a, &data_prefix(3)).is_empty());
    assert!(
        a.schema().get(SchemaMode::Query, &attr("name")).is_some(),
        "namespace 0 keeps its schema"
    );
}

#[when("everything in namespace 3 is dropped at 50")]
fn drop_ns_at_fifty(w: &mut Steps) {
    let a = fresh(w, "");
    fill_namespace_three(&a);
    next_commit_at(&a, 50);
    w.ts = vec![drop_op(&a, DropOp::AllInNs, "3").unwrap()];
}

#[then("a read at 60 finds namespace 3's initial schema and types")]
fn initial_at_sixty(w: &mut Steps) {
    let a = applier(w);
    assert_eq!(w.ts, vec![50]);
    next_commit_at(a, 61);
    let snapshot = a.lists().store().snapshot(60).unwrap();
    for s in initial_schema(3, false) {
        assert!(
            snapshot
                .get(&schema_key(&s.predicate).unwrap())
                .unwrap()
                .is_some(),
            "{}",
            s.predicate
        );
    }
    for t in initial_types(3, false) {
        assert!(
            snapshot
                .get(&type_key(&t.type_name).unwrap())
                .unwrap()
                .is_some(),
            "{}",
            t.type_name
        );
    }
    assert!(
        snapshot
            .get(&schema_key(&namespace_attr(3, "name")).unwrap())
            .unwrap()
            .is_none()
    );
}

#[when("everything is dropped")]
fn drop_everything(w: &mut Steps) {
    let a = fresh(w, "name: string @index(exact) .");
    with_types(&a, 0);
    ok(&a, vec![set_str("name", 1, "A")]);
    drop_op(&a, DropOp::All, "").unwrap();
}

#[then(
    "the store holds the root namespace's initial schema and types, written at timestamp 1, and nothing else"
)]
fn initial_only(w: &mut Steps) {
    let a = applier(w);
    let contents = a.lists().store().contents();
    let mut expected: Vec<Vec<u8>> = initial_schema(0, false)
        .iter()
        .map(|s| schema_key(&s.predicate).unwrap())
        .chain(
            initial_types(0, false)
                .iter()
                .map(|t| type_key(&t.type_name).unwrap()),
        )
        .collect();
    expected.sort();
    let keys: Vec<Vec<u8>> = contents.keys().keys().cloned().collect();
    assert_eq!(keys, expected);
    assert!(
        contents
            .keys()
            .values()
            .all(|v| v.len() == 1 && v[0].version() == 1)
    );
    assert_eq!(ns_contents(a.schema(), 0), initial_contents(0));
}

#[when("a namespace whose lists are split has its data dropped")]
fn drop_split_data(w: &mut Steps) {
    let a = splitting_applier(w);
    split_friend(&a);
    with_types(&a, 0);
    w.keys = stored_keys(&a, &[0x01]);
    w.keys.extend(stored_keys(&a, &[0x02]));
    drop_op(&a, DropOp::Data, "0").unwrap();
}

#[then(
    "no key under `0x00` or `0x04` of the namespace remains, and its schema and type keys are unchanged"
)]
fn split_data_dropped(w: &mut Steps) {
    // Dgraph's DeleteData drops the 0x00 prefix alone (ledger 7).
    let a = applier(w);
    assert!(stored_keys(a, &data_prefix(0)).is_empty());
    assert!(stored_keys(a, &twin(&data_prefix(0))).is_empty());
    let mut kept = stored_keys(a, &[0x01]);
    kept.extend(stored_keys(a, &[0x02]));
    assert_eq!(kept, w.keys);
}

#[when(
    "a request reads at 5, `name` is dropped at 8, and the request then commits a write to `name`"
)]
fn read_before_drop(w: &mut Steps) {
    let a = fresh(w, "name: string @index(exact) .");
    ok(&a, vec![set_str("name", 2, "B")]);
    next_commit_at(&a, 6);
    let mut batch = a.batch();
    let mut request = batch.request(5);
    request.run_mutation(&set_str("name", 1, "A")).unwrap();
    batch.push(request);
    next_commit_at(&a, 8);
    w.ts = vec![drop_predicate(&a, "name").unwrap()];
    w.result = Some(batch.commit().unwrap().remove(0));
}

#[then(
    "the commit fails with `Transaction has been aborted. Please retry`, and no key of `name` exists"
)]
fn read_before_drop_fails(w: &mut Steps) {
    assert_eq!(w.ts, vec![8]);
    let err = w.result.take().unwrap().unwrap_err();
    assert_eq!(
        err.to_string(),
        "Transaction has been aborted. Please retry"
    );
    let a = applier(w);
    let prefix = predicate_prefix(&attr("name")).unwrap();
    assert!(stored_keys(a, &prefix).is_empty());
    assert!(stored_keys(a, &schema_key(&attr("name")).unwrap()).is_empty());
}

#[when("a rollup of a `name` list reads at 7, `name` is dropped at 8, and the rollup then writes")]
fn rollup_across_drop(w: &mut Steps) {
    let probe = Probe::new(new_store());
    let seed = Seed::new(1);
    let cache = SimListCache::new(1024, seed.stream(Stream::Cache));
    let lists = Lists::new(probe.clone(), cache, ListsConfig::default());
    let a = Arc::new(
        Applier::new(
            lists,
            SteppedExecutor::new(seed.stream(Stream::Executor)),
            7,
        )
        .unwrap(),
    );
    a.alter(updates("name: string @index(exact) .")).unwrap();
    let m = |edges: Vec<DirectedEdge>| Mutations {
        start_ts: a.lists().latest_ts(),
        edges,
        ..Mutations::default()
    };
    a.apply_mutations(&m(vec![set_str("name", 1, "A")]))
        .unwrap();
    a.apply_mutations(&m(vec![set_str("name", 1, "B")]))
        .unwrap();
    let key = dkey("name", 1);
    let dropped = Arc::new(Mutex::new(None));
    let (inner, record) = (Arc::clone(&a), Arc::clone(&dropped));
    probe.on_next_snapshot(Box::new(move || {
        let m = Mutations {
            start_ts: inner.lists().latest_ts(),
            edges: vec![DirectedEdge {
                attr: attr("name"),
                value: b"_STAR_ALL".to_vec(),
                op: ultragraphdb_proto_pb::pb::directed_edge::Op::Del as i32,
                ..DirectedEdge::default()
            }],
            ..Mutations::default()
        };
        *record.lock().unwrap() = Some(inner.apply_mutations(&m).is_ok());
    }));
    w.flags = vec![a.lists().rollup_key(&key).unwrap()];
    w.flags.push(dropped.lock().unwrap().unwrap_or(false));
    let prefix = predicate_prefix(&attr("name")).unwrap();
    w.flags.push(
        probe
            .inner()
            .contents()
            .keys()
            .keys()
            .any(|k| k.starts_with(&prefix)),
    );
}

#[then("its batch is discarded, and no key of `name` exists")]
fn rollup_discarded(w: &mut Steps) {
    // Dgraph's drops do not stop rollups, so the rollup writes the list back (ledger 38).
    assert_eq!(w.flags, vec![false, true, false]);
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_test_bdd::features_dir()),
    );
}
