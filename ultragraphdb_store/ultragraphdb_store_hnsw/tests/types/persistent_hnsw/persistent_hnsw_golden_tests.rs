/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Whole runs of the index against Dgraph's (`tools/golden/hnsw.go`, `hnsw_graph`): the same inserts with the same
//! level draws, deletes and searches, over the same in-memory transaction. Go's uid `n` is the ID numbered `n`
//! (type 1, bucket 0, sequence `n`), which keeps Go's order, so the graph is Go's; Go's values are translated to the
//! 128-bit layouts before they are compared: a neighbour matrix's words to IDs, the entry point's 8 bytes to the ID's
//! 16, the dead list's decimal uids to the IDs' 32 hexadecimal digits, the entry's and the dead list's entity 1 to the
//! predicate's ID. Every insert's values, the store's contents after the run and every search's result must equal
//! Go's, translated.

use std::collections::BTreeMap;

use serde::Deserialize;
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_tok::{VEC_DEAD, VEC_ENTRY, VEC_KEYWORD};
use ultragraphdb_store_hnsw::{HnswFloat, PersistentHnsw, QueryCache, TxnCache, add_dead_node};
use ultragraphdb_tool_bdd::{Context, Golden, TestResult, load_golden, testdata_dir};
use ultragraphdb_traits_index::{KeyValue, VectorIndexOptions, accept_all};

use crate::utils::helpers::{
    MapTxn, draws, hex_id, id, ids, key, opts, translated, vec_bytes, vkeys,
};

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

pub fn graph_golden() -> TestResult<Golden<GraphCase>> {
    load_golden::<GraphCase>(&testdata_dir()?.join("golden/hnsw_graph.json"))
}

fn unhex(text: &str) -> TestResult<Vec<u8>> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2).context("two digits")?, 16).context("hex"))
        .collect()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Go's neighbour matrix, its words little-endian u64s: the row count, then per row its length and its uids.
fn go_matrix(bytes: &[u8]) -> TestResult<Vec<Vec<u64>>> {
    let words: Vec<u64> = bytes
        .chunks(8)
        .map(|w| Ok(u64::from_le_bytes(w.try_into().context("a word")?)))
        .collect::<TestResult<_>>()?;
    let mut it = words.into_iter();
    let rows = it.next().unwrap_or(0);
    (0..rows)
        .map(|_| {
            let len = it.next().context("a row's length")?;
            Ok(it.by_ref().take(usize::try_from(len)?).collect())
        })
        .collect()
}

/// A matrix of IDs in the port's layout, written from it apart from the encoder: the row count and each row's length
/// as little-endian u64s, each ID as the 16 little-endian bytes of its 128-bit value; no rows, no bytes.
fn id_matrix(rows: &[Vec<u64>]) -> TestResult<Vec<u8>> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = (rows.len() as u64).to_le_bytes().to_vec();
    for row in rows {
        out.extend_from_slice(&(row.len() as u64).to_le_bytes());
        for &n in row {
            out.extend_from_slice(&id(n)?.to_u128().to_le_bytes());
        }
    }
    Ok(out)
}

/// Go's value of an index list of `attr`, translated to the port's layout: a neighbour matrix's uids to IDs, the
/// entry point's 8 big-endian bytes to its ID's 16, the dead list's decimal uids to a JSON array of the IDs' 32
/// hexadecimal digits; a node's vector as it is.
pub fn go_value(attr: &str, value: &[u8]) -> TestResult<Vec<u8>> {
    if attr.ends_with(VEC_ENTRY) {
        let uid = u64::from_be_bytes(value.try_into().context("an 8-byte entry")?);
        return Ok(id(uid)?.to_be_bytes().to_vec());
    }
    if attr.ends_with(VEC_DEAD) {
        let text = std::str::from_utf8(value)?;
        let uids: Vec<u64> = text
            .trim_matches(['[', ']'])
            .split(',')
            .filter(|n| !n.is_empty())
            .map(|n| Ok(n.parse::<u64>()?))
            .collect::<TestResult<_>>()?;
        let listed: Vec<String> = ids(&uids)?
            .into_iter()
            .map(|id| format!("\"{}\"", hex_id(id)))
            .collect();
        return Ok(format!("[{}]", listed.join(",")).into_bytes());
    }
    if attr.ends_with(VEC_KEYWORD) {
        return id_matrix(&go_matrix(value)?);
    }
    Ok(value.to_vec())
}

/// Go's names of an insert's values, `attr/entity/value hex`, translated to the port's (see [`golden_edges`]).
pub fn go_edges(c: &GraphCase, op: &GraphOp) -> TestResult<Vec<String>> {
    let mut names: Vec<String> = op
        .edges
        .iter()
        .flatten()
        .map(|e| {
            let mut parts = e.splitn(3, '/');
            let (attr, entity, value) = (
                parts.next().context("an attribute")?,
                parts.next().context("an entity")?,
                parts.next().context("a value")?,
            );
            let entity = if attr.ends_with(VEC_KEYWORD) {
                id(entity.parse()?)?
            } else {
                vkeys(&c.pred)?.predicate()
            };
            let value = go_value(attr, &unhex(value)?)?;
            Ok(format!("{attr}/{}/{}", hex_id(entity), hex(&value)))
        })
        .collect::<TestResult<_>>()?;
    names.sort();
    Ok(names)
}

/// Go's contents after a run, its keys translated to the quad keys and its values to the port's layouts.
pub fn go_contents(c: &GraphCase) -> TestResult<BTreeMap<String, String>> {
    c.contents
        .iter()
        .map(|(k, v)| {
            let dgraph_key = unhex(k)?;
            let key = translated(&dgraph_key)
                .ok_or_else(|| format!("{}: a Dgraph key {k}", c.name))
                .context("the Dgraph key translates")?;
            let len = usize::from(u16::from_be_bytes(
                dgraph_key.get(9..11).context("a length")?.try_into()?,
            ));
            let attr = std::str::from_utf8(dgraph_key.get(11..11 + len).context("an attribute")?)?;
            Ok((hex(&key), hex(&go_value(attr, &unhex(v)?)?)))
        })
        .collect()
}

/// Go's result of a search, its uids the IDs numbered by them.
pub fn go_result(op: &GraphOp) -> TestResult<Vec<SystemId>> {
    Ok(ids(&op.result.clone().unwrap_or_default())?)
}

/// An op's vector at the run's width.
pub fn vector<T: HnswFloat>(op: &GraphOp) -> TestResult<Vec<T>> {
    op.vector
        .iter()
        .flatten()
        .map(|&bits| {
            Ok(if T::BITS == 32 {
                T::from_f32(f32::from_bits(u32::try_from(bits)?))
            } else {
                T::from_f64(f64::from_bits(bits))
            })
        })
        .collect()
}

/// The names of an insert's values, `attr/entity/value hex`, the entity its ID's 32 hexadecimal digits, sorted.
pub fn golden_edges(edges: &[KeyValue]) -> Vec<String> {
    let mut names: Vec<String> = edges
        .iter()
        .map(|e| format!("{}/{}/{}", e.attr(), hex_id(e.entity()), hex(e.value())))
        .collect();
    names.sort();
    names
}

/// Runs the case; whether it stopped where Go's entry point hands over (see [`hands_over`]).
fn run<T: HnswFloat>(c: &GraphCase) -> TestResult<bool> {
    let pairs: Vec<(&str, &str)> = c
        .options
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let o = opts(&pairs)?;
    let txn = MapTxn::default();
    let mut draw = draws(c.draws.iter().map(|&b| f64::from_bits(b)).collect());
    let dead_key = key(&format!("{}{VEC_DEAD}", c.pred), 1)?;
    let mut searched = 0;
    let mut search_index = None;
    for (i, op) in c.ops.iter().enumerate() {
        let at = format!("{} op {i} ({} {})", c.name, op.op, op.uid);
        match op.op.as_str() {
            "insert" => {
                let v = vector::<T>(op)?;
                txn.put(key(&c.pred, op.uid)?, vec_bytes(&v));
                let index = PersistentHnsw::<T>::new(vkeys(&c.pred)?, &o, c.bits)?;
                let edges = index.insert(&TxnCache::new(&txn, 1), id(op.uid)?, &v, &mut draw);
                if let Some(heir) = go_hands_over(c, op) {
                    hands_over(c, op, heir, &edges?)?;
                    return Ok(true);
                }
                match edges {
                    Ok(edges) => {
                        assert_eq!(golden_edges(&edges), go_edges(c, op)?, "{at}");
                    }
                    Err(e) => assert_eq!(e.to_string(), op.error, "{at}"),
                }
            }
            "delete" => {
                txn.remove(&key(&c.pred, op.uid)?);
                let existing = txn.db.borrow().get(&dead_key).cloned();
                let dead = add_dead_node(existing.as_deref(), id(op.uid)?)?;
                txn.put(dead_key.clone(), dead);
            }
            search => {
                if search_index.is_none() {
                    search_index = Some(PersistentHnsw::<T>::new(vkeys(&c.pred)?, &o, c.bits)?);
                }
                if let Some(index) = &search_index {
                    run_search(index, &txn, op, search, &at)?;
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
    // Go's keys are Dgraph's: each is compared under its translation to the vector-index and forward keys.
    assert_eq!(
        contents,
        go_contents(c)?,
        "{}: the store after the run",
        c.name
    );
    assert!(searched > 50, "{}", c.name);
    Ok(false)
}

/// Runs `search`, the case's op `op`, through `index` over `txn`, and checks it against Go's result; `at` names the op.
fn run_search<T: HnswFloat>(
    index: &PersistentHnsw<T>,
    txn: &MapTxn,
    op: &GraphOp,
    search: &str,
    at: &str,
) -> TestResult {
    let qc = QueryCache::new(txn, 1);
    let query = vector::<T>(op)?;
    let result = match search {
        "search" => Ok(index.search(&qc, &query, op.k, accept_all::<T>)),
        "search_options" => Ok(index.search_with_options(
            &qc,
            &query,
            op.k,
            &VectorIndexOptions::new(op.ef_override, op.threshold, accept_all::<T>),
        )),
        "search_uid" => Ok(index.search_with_uid(&qc, id(op.uid)?, op.k, accept_all::<T>)),
        "search_uid_options" => Ok(index.search_with_uid_and_options(
            &qc,
            id(op.uid)?,
            op.k,
            &VectorIndexOptions::new(op.ef_override, op.threshold, accept_all::<T>),
        )),
        other => Err(format!("unknown op {other}")),
    }
    .context("the op is a known search")?;
    match result {
        Ok(uids) => {
            assert!(op.error.is_empty(), "{at}: Go failed with {}", op.error);
            assert_eq!(uids, go_result(op)?, "{at}");
        }
        Err(e) => assert_eq!(e.to_string(), op.error, "{at}"),
    }
    Ok(())
}

/// The node Go's insert makes the entry point by wiping it, when the entry point lost its vector: the insert writes
/// exactly the entry naming another node and that node's empty lists, and nothing of the node inserted.
fn go_hands_over(c: &GraphCase, op: &GraphOp) -> Option<u64> {
    let go = op.edges.as_ref()?;
    let prefix = format!("{}__vector_entry/1/", c.pred);
    let heir = go.iter().find_map(|e| e.strip_prefix(&prefix))?;
    let heir = u64::from_str_radix(heir, 16).ok()?;
    (heir != op.uid && go.len() == 2).then_some(heir)
}

/// Deviation (ledger 45): in `dead` and `dead_entry`, uid 3, the entry point, loses its vector, and Go's next insert
/// rewrites the first node holding a vector with empty lists at every level and returns without inserting its node.
/// The port names that node the entry point, keeps its lists and inserts the node from it, so the run follows Go's
/// up to that insert and its own after it.
fn hands_over(c: &GraphCase, op: &GraphOp, heir: u64, edges: &[KeyValue]) -> TestResult {
    let names = golden_edges(edges);
    let go = go_edges(c, op)?;
    let entry = format!(
        "{}__vector_entry/{}/{}",
        c.pred,
        hex_id(vkeys(&c.pred)?.predicate()),
        hex(&id(heir)?.to_be_bytes())
    );
    let lists_of = |uid: u64| -> TestResult<String> {
        Ok(format!("{}__vector_/{}/", c.pred, hex_id(id(uid)?)))
    };
    let (heir_lists, inserted_lists) = (lists_of(heir)?, lists_of(op.uid)?);
    // Empty lists at every level: rows of length 0, the matrix ending in at least two zero words.
    let wiped = |e: &String| e.starts_with(&heir_lists) && e.ends_with(&"0".repeat(32));
    assert!(
        go.contains(&entry) && go.iter().any(wiped),
        "{}: Go wipes {heir}: {go:?}",
        c.name
    );
    assert!(names.contains(&entry), "{}: {names:?}", c.name);
    assert!(
        !names.iter().any(wiped),
        "{}: {heir} keeps its lists: {names:?}",
        c.name
    );
    assert!(
        names.iter().any(|e| e.starts_with(&inserted_lists)),
        "{}: {} is inserted: {names:?}",
        c.name,
        op.uid
    );
    Ok(())
}

#[test]
fn test_graph_runs_equal_gos() -> TestResult {
    let golden = graph_golden()?;
    assert_eq!(golden.cases().len(), 10);
    let mut handed_over = Vec::new();
    for c in golden.cases() {
        let stopped = if c.bits == 32 {
            run::<f32>(c)?
        } else {
            run::<f64>(c)?
        };
        if stopped {
            handed_over.push(c.name.as_str());
        }
    }
    assert_eq!(handed_over, vec!["dead", "dead_entry"]);
    Ok(())
}
