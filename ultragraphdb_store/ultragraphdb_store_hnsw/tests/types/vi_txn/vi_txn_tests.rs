/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The index over the posting crate's local commit: values read through a request, a node's vector from its
//! forward list's encoding, written into its scratch layers and committed in its batch, on the simulated store and on
//! fjall; and the golden runs of 32-bit vectors replayed through it, Go's keys translated to the quad keys. A
//! `ugdb:float32vector` literal holds 32-bit floats, so the 64-bit runs replay over the golden transaction alone.

use std::collections::BTreeMap;

use ultragraphdb_core_tok::{VEC_DEAD, VEC_ENTRY, VEC_KEYWORD};
use ultragraphdb_store_hnsw::{HnswError, ViTxn, bytes_to_uint64, decode_uint64_matrix};
use ultragraphdb_store_kv::{Durability, FjallStore};
use ultragraphdb_store_posting::{CachedList, Lists};
use ultragraphdb_store_sim::SimListCache;
use ultragraphdb_traits_index::{KeyValue, LocalCache, Txn};
use ultragraphdb_traits_store::{Scan, Store, StoreSnapshot};

use crate::types::persistent_hnsw::persistent_hnsw_golden_tests::{
    GraphCase, golden_edges, graph_golden, vector,
};
use crate::utils::helpers::{
    delete, draws, index_value, insert, key, lists_over, new_lists, new_store, opts, pred,
    set_vector, translated, vkeys,
};

/// The bytes of `v`'s elements, little-endian.
fn elements(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

#[test]
fn test_reads_see_the_requests_writes_and_absence_is_no_value() {
    let lists = new_lists();
    let p = pred("emb");
    let mut request = lists.request(lists.fresh_ts());
    set_vector(&mut request, &p, 7, &[1.0, 2.0]).unwrap();
    let txn = ViTxn::new(&mut request);
    assert_eq!(Txn::get(&txn, &key(&p, 7)), Ok(elements(&[1.0, 2.0])));
    assert_eq!(Txn::get(&txn, &key(&p, 8)), Err(HnswError::NoValue()));
    assert_eq!(
        LocalCache::get_with_lock_held(&txn, &key(&p, 7)),
        Ok(elements(&[1.0, 2.0]))
    );
    let kv = KeyValue::new(1, format!("{p}{VEC_ENTRY}"), 7u64.to_be_bytes().to_vec());
    txn.add_mutation(&key(&format!("{p}{VEC_ENTRY}"), 1), &kv)
        .unwrap();
    txn.lock_key(b"any");
    txn.unlock_key(b"any");
    assert_eq!(
        Txn::get_with_lock_held(&txn, &key(&format!("{p}{VEC_ENTRY}"), 1)),
        Ok(7u64.to_be_bytes().to_vec())
    );
    assert_eq!(txn.start_ts(), txn.read_ts());
}

/// `find` scans the store at the request's timestamp, as Dgraph's `LocalCache.Find` scans Badger: the vectors of
/// earlier commits, in key order, through the request's own writes; not the request's new keys.
#[test]
fn test_find_scans_the_committed_vectors_in_key_order() {
    let lists = new_lists();
    let p = pred("emb");
    let o = opts(&[("maxLevels", "2")]);
    let mut draw = || 0.5;
    for uid in [9u64, 4, 6] {
        insert(&lists, &p, &o, uid, &[uid as f32, 1.0], &mut draw).unwrap();
    }
    let mut request = lists.request(lists.fresh_ts());
    set_vector(&mut request, &p, 2, &[2.0, 2.0]).unwrap();
    let txn = ViTxn::new(&mut request);
    let prefix = vkeys(&p).vector_prefix();
    assert_eq!(
        Txn::find(&txn, &prefix, |_| true),
        Ok(4),
        "uid 2 is not committed"
    );
    assert_eq!(
        LocalCache::find(&txn, &prefix, |v| v.len() == 8
            && v[..4] == 9f32.to_le_bytes()),
        Ok(9)
    );
    assert_eq!(
        Txn::find(&txn, &prefix, |_| false),
        Err(HnswError::KeyNotFound())
    );
    delete(&lists, &p, 4).unwrap();
    let mut request = lists.request(lists.fresh_ts());
    let txn = ViTxn::new(&mut request);
    assert_eq!(
        Txn::find(&txn, &prefix, |_| true),
        Ok(6),
        "a deleted vector is skipped"
    );
}

#[test]
fn test_an_insert_is_committed_with_its_request() {
    let lists = new_lists();
    let p = pred("emb");
    let o = opts(&[("maxLevels", "3")]);
    let edges = insert(&lists, &p, &o, 5, &[0.5f32, 0.5], &mut || 0.5).unwrap();
    assert_eq!(edges.len(), 2);
    let ts = lists.latest_ts();
    let entry = index_value(&lists, &key(&format!("{p}{VEC_ENTRY}"), 1), ts).unwrap();
    assert_eq!(bytes_to_uint64(&entry), Some(5));
    let node = index_value(&lists, &key(&format!("{p}{VEC_KEYWORD}"), 5), ts).unwrap();
    assert_eq!(decode_uint64_matrix(&node), Ok(vec![vec![]; 3]));
    assert_eq!(
        index_value(&lists, &key(&format!("{p}{VEC_ENTRY}"), 1), ts - 1),
        None,
        "not before its commit"
    );
    insert(&lists, &p, &o, 8, &[0.4f32, 0.5], &mut || 0.5).unwrap();
    let node = index_value(
        &lists,
        &key(&format!("{p}{VEC_KEYWORD}"), 5),
        lists.latest_ts(),
    )
    .unwrap();
    assert_eq!(
        decode_uint64_matrix(&node),
        Ok(vec![vec![], vec![], vec![8]])
    );
    delete(&lists, &p, 8).unwrap();
    let dead = index_value(
        &lists,
        &key(&format!("{p}{VEC_DEAD}"), 1),
        lists.latest_ts(),
    )
    .unwrap();
    assert_eq!(dead, b"[8]");
}

/// Replays a golden run through the posting lists of `store`, each insert in a request committed on its own, and
/// compares the values, the results and the index's stored state with Go's.
fn replay<S: Store>(c: &GraphCase, lists: &Lists<S, SimListCache<CachedList>>) {
    let pairs: Vec<(&str, &str)> = c
        .options
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let o = opts(&pairs);
    let mut draw = draws(c.draws.iter().map(|&b| f64::from_bits(b)).collect());
    for (i, op) in c.ops.iter().enumerate() {
        match op.op.as_str() {
            "insert" => {
                let edges =
                    insert(lists, &c.pred, &o, op.uid, &vector::<f32>(op), &mut draw).unwrap();
                assert_eq!(
                    &golden_edges(&edges),
                    op.edges.as_ref().unwrap(),
                    "{} op {i}",
                    c.name
                );
            }
            "search" => {
                let got = crate::utils::helpers::search(
                    lists,
                    &c.pred,
                    &o,
                    &vector::<f32>(op),
                    op.k,
                    None,
                )
                .unwrap();
                assert_eq!(
                    got,
                    op.result.clone().unwrap_or_default(),
                    "{} op {i}",
                    c.name
                );
            }
            _ => {}
        }
    }
    let ts = lists.latest_ts();
    let snapshot = lists.store().snapshot(ts).unwrap();
    let mut stored = BTreeMap::new();
    for item in snapshot.scan(&Scan::prefix(Vec::new())) {
        let k = item.unwrap().key().to_vec();
        if let Some(v) = index_value(lists, &k, ts) {
            stored.insert(hex(&k), hex(&v));
        }
    }
    let expected: BTreeMap<String, String> = c
        .contents
        .iter()
        .map(|(k, v)| {
            (
                hex(&translated(&unhex(k)).expect("a Dgraph key")),
                v.clone(),
            )
        })
        .collect();
    assert_eq!(stored, expected, "{}: the store after the run", c.name);
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The runs of 32-bit vectors whose entry point never loses its vector: in the others the new entry point is found
/// by `find`, which Dgraph runs over the committed store and the golden's transaction over its one map.
fn replayable(c: &GraphCase) -> bool {
    c.bits == 32 && !c.ops.iter().any(|op| op.op == "delete")
}

#[test]
fn test_golden_runs_through_the_posting_lists_equal_gos() {
    let golden = graph_golden();
    let mut replayed = 0;
    for c in golden.cases().iter().filter(|c| replayable(c)) {
        replay(c, &lists_over(new_store()));
        replayed += 1;
    }
    assert_eq!(replayed, 6);
}

#[test]
fn test_a_golden_run_on_fjall_equals_gos() {
    let root =
        std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, std::path::PathBuf::from);
    let dir = root.join(format!("hnsw-golden-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let golden = graph_golden();
    let c = golden.cases().iter().find(|c| c.name == "cosine").unwrap();
    let store = FjallStore::open(&dir, Durability::Process).expect("the store opens");
    replay(c, &lists_over(store));
    let _ = std::fs::remove_dir_all(&dir);
}
