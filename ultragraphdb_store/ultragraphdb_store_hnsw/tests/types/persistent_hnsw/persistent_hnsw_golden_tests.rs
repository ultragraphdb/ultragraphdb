/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Whole runs of the index against Dgraph's (`tools/golden/hnsw.go`, `hnsw_graph`): the same inserts with the same
//! level draws, deletes and searches, over the same in-memory transaction. Every insert's values, the store's
//! contents after the run and every search's result must equal Go's.

use std::collections::BTreeMap;

use serde::Deserialize;
use ultragraphdb_core_tok::VEC_DEAD;
use ultragraphdb_store_hnsw::{HnswFloat, PersistentHnsw, QueryCache, TxnCache, add_dead_node};
use ultragraphdb_test_bdd::{Golden, load_golden, testdata_dir};
use ultragraphdb_traits_index::{KeyValue, VectorIndexOptions, accept_all};

use crate::utils::helpers::{MapTxn, draws, key, opts, vec_bytes};

#[derive(Debug, Clone, Deserialize)]
pub struct GraphOp {
    pub op: String,
    pub uid: u64,
    pub vector: Option<Vec<u64>>,
    pub k: usize,
    pub ef_override: usize,
    pub threshold: Option<f64>,
    pub edges: Option<Vec<String>>,
    pub result: Option<Vec<u64>>,
    pub error: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphCase {
    pub name: String,
    pub bits: usize,
    pub pred: String,
    pub options: BTreeMap<String, String>,
    pub draws: Vec<u64>,
    pub ops: Vec<GraphOp>,
    pub contents: BTreeMap<String, String>,
}

pub fn graph_golden() -> Golden<GraphCase> {
    load_golden::<GraphCase>(&testdata_dir().join("golden/hnsw_graph.json"))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// An op's vector at the run's width.
pub fn vector<T: HnswFloat>(op: &GraphOp) -> Vec<T> {
    op.vector
        .iter()
        .flatten()
        .map(|&bits| {
            if T::BITS == 32 {
                T::from_f32(f32::from_bits(bits as u32))
            } else {
                T::from_f64(f64::from_bits(bits))
            }
        })
        .collect()
}

/// The golden's names of an insert's values, `attr/entity/value hex`, sorted.
pub fn golden_edges(edges: &[KeyValue]) -> Vec<String> {
    let mut names: Vec<String> = edges
        .iter()
        .map(|e| format!("{}/{}/{}", e.attr(), e.entity(), hex(e.value())))
        .collect();
    names.sort();
    names
}

fn run<T: HnswFloat>(c: &GraphCase) {
    let pairs: Vec<(&str, &str)> = c
        .options
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let o = opts(&pairs);
    let txn = MapTxn::default();
    let mut draw = draws(c.draws.iter().map(|&b| f64::from_bits(b)).collect());
    let dead_key = key(&format!("{}{VEC_DEAD}", c.pred), 1);
    let mut searched = 0;
    let mut search_index = None;
    for (i, op) in c.ops.iter().enumerate() {
        let at = format!("{} op {i} ({} {})", c.name, op.op, op.uid);
        match op.op.as_str() {
            "insert" => {
                let v = vector::<T>(op);
                txn.db
                    .borrow_mut()
                    .insert(key(&c.pred, op.uid), vec_bytes(&v));
                let index = PersistentHnsw::<T>::new(&c.pred, &o, c.bits).unwrap();
                let edges = index.insert(&TxnCache::new(&txn, 1), op.uid, &v, &mut draw);
                match edges {
                    Ok(edges) => {
                        assert_eq!(&golden_edges(&edges), op.edges.as_ref().unwrap(), "{at}");
                    }
                    Err(e) => assert_eq!(e.to_string(), op.error, "{at}"),
                }
            }
            "delete" => {
                txn.db.borrow_mut().remove(&key(&c.pred, op.uid));
                let existing = txn.db.borrow().get(&dead_key).cloned();
                let dead = add_dead_node(existing.as_deref(), op.uid).unwrap();
                txn.db.borrow_mut().insert(dead_key.clone(), dead);
            }
            search => {
                let index = search_index
                    .get_or_insert_with(|| PersistentHnsw::<T>::new(&c.pred, &o, c.bits).unwrap());
                let qc = QueryCache::new(&txn, 1);
                let query = vector::<T>(op);
                let result = match search {
                    "search" => index.search(&qc, &query, op.k, accept_all::<T>),
                    "search_options" => index.search_with_options(
                        &qc,
                        &query,
                        op.k,
                        &VectorIndexOptions::new(op.ef_override, op.threshold, accept_all::<T>),
                    ),
                    "search_uid" => index.search_with_uid(&qc, op.uid, op.k, accept_all::<T>),
                    "search_uid_options" => index.search_with_uid_and_options(
                        &qc,
                        op.uid,
                        op.k,
                        &VectorIndexOptions::new(op.ef_override, op.threshold, accept_all::<T>),
                    ),
                    other => panic!("unknown op {other}"),
                };
                match result {
                    Ok(uids) => {
                        assert!(op.error.is_empty(), "{at}: Go failed with {}", op.error);
                        assert_eq!(uids, op.result.clone().unwrap_or_default(), "{at}");
                    }
                    Err(e) => assert_eq!(e.to_string(), op.error, "{at}"),
                }
                searched += 1;
            }
        }
    }
    let contents: BTreeMap<String, String> = txn
        .db
        .borrow()
        .iter()
        .map(|(k, v)| (hex(k), hex(v)))
        .collect();
    assert_eq!(contents, c.contents, "{}: the store after the run", c.name);
    assert!(searched > 50, "{}", c.name);
}

#[test]
fn test_graph_runs_equal_gos() {
    let golden = graph_golden();
    assert_eq!(golden.cases().len(), 10);
    for c in golden.cases() {
        if c.bits == 32 {
            run::<f32>(c);
        } else {
            run::<f64>(c);
        }
    }
}
