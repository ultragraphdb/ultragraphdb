/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The vector-index scenarios, run over the posting lists of the simulated store: every insert and delete in a
//! request committed on its own, as index maintenance makes them, every search through a request at the newest
//! timestamp.

use cucumber::{World, then, when};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{Graph, Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_tok::{VEC_DEAD, VEC_ENTRY, VEC_KEYWORD};
use ultragraphdb_store_hnsw::{HnswError, VectorKeys, bytes_to_id, decode_id_matrix, parse_edges};
use ultragraphdb_store_sim::SimRng;
use ultragraphdb_traits_index::{Float, KeyValue, Metric, VectorIndexOptions, accept_all};
use ultragraphdb_traits_store::{Scan, Store, StoreSnapshot};

/// The unit tests' helpers, shared by path.
pub mod utils {
    #[path = "helpers.rs"]
    pub mod helpers;
}

use crate::utils::helpers::{
    TestLists, brute_force, delete, draw, hex_id, id, index_value, insert, insert_at, key,
    new_lists, nums, opts, oracle_score, pred, search, vkeys,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// What a search of an empty index reported, and the neighbours its caller took from it.
type EmptySearch = (Result<Vec<u64>, HnswError>, Vec<u64>);

#[derive(Debug, Default, World)]
struct Steps {
    lists: Option<TestLists>,
    pred: String,
    uid: u64,
    later_edges: Vec<KeyValue>,
    compared: Vec<(Metric, Vec<u64>, Vec<u64>)>,
    within: Vec<(Metric, f64, Vec<f64>)>,
    within_exact: Vec<(Metric, Vec<u64>, Vec<u64>)>,
    empty: Option<EmptySearch>,
    heir_lists: Vec<Vec<u64>>,
    /// Each index of two graphs and predicates, with the nodes it holds and the nodes a search of it found.
    apart: Vec<(VectorKeys, Vec<u64>, Vec<u64>)>,
}

fn lists(w: &Steps) -> TestResult<&TestLists> {
    w.lists.as_ref().context("lists")
}

fn widen(v: &[f32]) -> Vec<f64> {
    v.iter().map(|&f| f64::from(f)).collect()
}

fn widened(vectors: &[(u64, Vec<f32>)]) -> TestResult<Vec<(SystemId, Vec<f64>)>> {
    vectors
        .iter()
        .map(|(uid, v)| Ok((id(*uid)?, widen(v))))
        .collect()
}

/// The rows of a stored neighbour matrix, each node by its number.
fn rows(matrix: &[u8]) -> TestResult<Vec<Vec<u64>>> {
    let decoded = decode_id_matrix(matrix).map_err(ultragraphdb_tool_bdd::TestError::new)?;
    Ok(decoded.iter().map(|row| nums(row)).collect())
}

fn random_vector(rng: &mut SimRng, dims: usize) -> Vec<f32> {
    (0..dims)
        .map(|_| f32::from_f64(draw(rng) * 2.0 - 1.0))
        .collect()
}

#[when("the first vector is inserted into an empty index")]
fn first_insert(w: &mut Steps) -> TestResult {
    w.lists = Some(new_lists());
    w.pred = pred("emb");
    w.uid = 7;
    let _inserted: Vec<KeyValue> = insert(
        lists(w)?,
        &w.pred,
        &opts(&[("maxLevels", "3")])?,
        id(7)?,
        &[0.5f32, 0.25],
        &mut || 0.5,
    )?;
    Ok(())
}

#[then(
    "the entry point names its node's system ID and its neighbour lists exist, empty, at every level"
)]
fn entry_and_empty_lists(w: &mut Steps) -> TestResult {
    let ts = lists(w)?.applied_ts();
    let entry = index_value(lists(w)?, &key(&format!("{}{VEC_ENTRY}", w.pred), 1)?, ts)?
        .context("index_value(..)?")?;
    // The node's ID as its 16 big-endian bytes.
    assert_eq!(entry, id(w.uid)?.to_be_bytes());
    assert_eq!(bytes_to_id(&entry), Some(id(w.uid)?));
    let node = index_value(
        lists(w)?,
        &key(&format!("{}{VEC_KEYWORD}", w.pred), w.uid)?,
        ts,
    )?
    .context("index_value(..)?")?;
    assert_eq!(rows(&node)?, vec![Vec::<u64>::new(); 3]);
    Ok(())
}

#[when("a vector is deleted")]
fn vector_deleted(w: &mut Steps) -> TestResult {
    w.lists = Some(new_lists());
    w.pred = pred("emb");
    let o = opts(&[
        ("maxLevels", "3"),
        ("efConstruction", "6"),
        ("efSearch", "6"),
    ])?;
    let mut rng = SimRng::new(3);
    for uid in 1..=20 {
        let v = random_vector(&mut rng, 3);
        let _inserted: Vec<KeyValue> =
            insert(lists(w)?, &w.pred, &o, id(uid)?, &v, &mut || draw(&mut rng))?;
    }
    // Uid 1 is the entry point; deleting another leaves the graph rooted.
    w.uid = 9;
    delete(lists(w)?, &vkeys(&w.pred)?, id(w.uid)?)?;
    for uid in 21..=40 {
        let v = random_vector(&mut rng, 3);
        let edges = insert(lists(w)?, &w.pred, &o, id(uid)?, &v, &mut || draw(&mut rng))?;
        w.later_edges.extend(edges);
    }
    Ok(())
}

#[then(
    "its node's ID is added to the dead list as 32 hexadecimal digits, and later inserts leave it out of the neighbour lists they write"
)]
fn dead_and_left_out(w: &mut Steps) -> TestResult {
    let ts = lists(w)?.applied_ts();
    let dead = index_value(lists(w)?, &key(&format!("{}{VEC_DEAD}", w.pred), 1)?, ts)?
        .context("index_value(..)?")?;
    let digits = hex_id(id(w.uid)?);
    assert_eq!(digits.len(), 32);
    assert_eq!(dead, format!("[\"{digits}\"]").into_bytes());
    assert_eq!(
        parse_edges(&String::from_utf8_lossy(&dead))?,
        vec![id(w.uid)?]
    );
    let vec_key = format!("{}{VEC_KEYWORD}", w.pred);
    let written: Vec<&KeyValue> = w
        .later_edges
        .iter()
        .filter(|e| e.attr() == vec_key)
        .collect();
    assert!(
        written.len() > 20,
        "the later inserts wrote {} lists",
        written.len()
    );
    for e in written {
        let levels = rows(e.value())?;
        assert!(
            !levels.iter().flatten().any(|&u| u == w.uid),
            "the lists of {} name {}",
            hex_id(e.entity()),
            w.uid
        );
    }
    Ok(())
}

#[when(
    "50 vectors are indexed under each metric and a search asks for the 5 nearest to a query vector"
)]
fn fifty_per_metric(w: &mut Steps) -> TestResult {
    for metric in [Metric::Euclidean, Metric::Cosine, Metric::DotProduct] {
        let lists = new_lists();
        let p = pred("emb");
        let o = opts(&[("metric", metric.name())])?;
        let mut rng = SimRng::new(50);
        let mut vectors = Vec::new();
        for uid in 1..=50 {
            let v = random_vector(&mut rng, 4);
            let _inserted: Vec<KeyValue> =
                insert(&lists, &p, &o, id(uid)?, &v, &mut || draw(&mut rng))?;
            vectors.push((uid, v));
        }
        let query = random_vector(&mut rng, 4);
        let found = search(
            &lists,
            &vkeys(&p)?,
            &o,
            &query,
            5,
            Some(&VectorIndexOptions::default()),
        )?;
        // The exhaustive oracle, its distances computed naively in float64, not by the index's kernels.
        let brute = brute_force(metric, &widened(&vectors)?, &widen(&query), 5);
        w.compared.push((metric, nums(&found), nums(&brute)));
    }
    Ok(())
}

#[then("the result equals the 5 nearest by brute force under that metric")]
fn equals_brute_force(w: &mut Steps) {
    assert_eq!(w.compared.len(), 3);
    for (metric, found, brute) in &w.compared {
        assert_eq!(found, brute, "{metric:?}");
    }
}

#[when("the entry point's vector is deleted and another vector is inserted")]
fn entry_deleted(w: &mut Steps) -> TestResult {
    w.lists = Some(new_lists());
    w.pred = pred("emb");
    let o = opts(&[
        ("maxLevels", "3"),
        ("efConstruction", "6"),
        ("efSearch", "6"),
    ])?;
    let mut rng = SimRng::new(5);
    for uid in 1..=12 {
        let v = random_vector(&mut rng, 3);
        let _inserted: Vec<KeyValue> =
            insert(lists(w)?, &w.pred, &o, id(uid)?, &v, &mut || draw(&mut rng))?;
    }
    let ts = lists(w)?.applied_ts();
    let heir_key = key(&format!("{}{VEC_KEYWORD}", w.pred), 2)?;
    w.heir_lists = rows(
        &index_value(lists(w)?, &heir_key, ts)?
            .context("index_value(lists(w)?, &heir_key, ts)?")?,
    )?;
    // Node 1 is the entry point; node 2 is the first node holding a vector after it.
    delete(lists(w)?, &vkeys(&w.pred)?, id(1)?)?;
    w.uid = 13;
    let v = random_vector(&mut rng, 3);
    let _inserted: Vec<KeyValue> =
        insert(lists(w)?, &w.pred, &o, id(13)?, &v, &mut || draw(&mut rng))?;
    Ok(())
}

#[then(
    "the first node holding a vector becomes the entry point with its neighbour lists kept, and the new vector is inserted"
)]
fn handed_over(w: &mut Steps) -> TestResult {
    // Dgraph rewrites uid 2 with empty lists at every level and does not insert 13 (ledger 45).
    let ts = lists(w)?.applied_ts();
    let entry = index_value(lists(w)?, &key(&format!("{}{VEC_ENTRY}", w.pred), 1)?, ts)?
        .context("index_value(..)?")?;
    assert_eq!(bytes_to_id(&entry), Some(id(2)?));
    let vec_key = format!("{}{VEC_KEYWORD}", w.pred);
    let heir = rows(&index_value(lists(w)?, &key(&vec_key, 2)?, ts)?.context("index_value(..)?")?)?;
    assert!(heir.iter().any(|l| !l.is_empty()), "{heir:?}");
    for (before, after) in w.heir_lists.iter().zip(&heir) {
        assert!(
            before
                .iter()
                .filter(|&&u| u != 1)
                .all(|u| after.contains(u)),
            "{before:?} -> {after:?}"
        );
    }
    let new =
        rows(&index_value(lists(w)?, &key(&vec_key, w.uid)?, ts)?.context("index_value(..)?")?)?;
    assert!(new.iter().any(|l| !l.is_empty()), "13 is linked: {new:?}");
    Ok(())
}

#[when("a search carries a distance threshold")]
fn threshold_search(w: &mut Steps) -> TestResult {
    for (metric, threshold) in [(Metric::Euclidean, 0.7), (Metric::Cosine, 0.2)] {
        let lists = new_lists();
        let p = pred("emb");
        let o = opts(&[("metric", metric.name())])?;
        let mut rng = SimRng::new(70);
        let mut vectors = Vec::new();
        for uid in 1..=40 {
            let v = random_vector(&mut rng, 3);
            let _inserted: Vec<KeyValue> =
                insert(&lists, &p, &o, id(uid)?, &v, &mut || draw(&mut rng))?;
            vectors.push((uid, v));
        }
        let query = random_vector(&mut rng, 3);
        let options = VectorIndexOptions::new(0, Some(threshold), accept_all::<f32>);
        let found = nums(&search(
            &lists,
            &vkeys(&p)?,
            &o,
            &query,
            40,
            Some(&options),
        )?);
        // The exhaustive oracle: every vector within the threshold, best first. The count asked for is the whole set and
        // the default efSearch of 90 exceeds it, so the search is exact.
        let oracle_distance = |v: &[f32]| {
            let score = oracle_score(metric, &widen(v), &widen(&query));
            if metric == Metric::Cosine {
                1.0 - score
            } else {
                score
            }
        };
        let vector_of = |uid: u64| {
            vectors
                .iter()
                .find(|(u, _)| *u == uid)
                .map(|(_, v)| v.as_slice())
                .context("the vector of a found uid")
        };
        let mut exact = Vec::new();
        for uid in nums(&brute_force(
            metric,
            &widened(&vectors)?,
            &widen(&query),
            40,
        )) {
            if oracle_distance(vector_of(uid)?) <= threshold {
                exact.push(uid);
            }
        }
        w.within_exact.push((metric, found.clone(), exact));
        let distances = found
            .iter()
            .map(|uid| Ok(oracle_distance(vector_of(*uid)?)))
            .collect::<TestResult<Vec<_>>>()?;
        w.within.push((metric, threshold, distances));
    }
    Ok(())
}

#[then("no returned vector lies farther than the threshold")]
fn within_threshold(w: &mut Steps) {
    for (metric, threshold, distances) in &w.within {
        assert!(
            !distances.is_empty(),
            "{metric:?}: something lies within {threshold}"
        );
        assert!(
            distances.iter().all(|d| d <= threshold),
            "{metric:?}: {distances:?} beyond {threshold}"
        );
    }
    // Nor does the threshold hold back a vector within it.
    assert_eq!(w.within_exact.len(), 2);
    for (metric, found, exact) in &w.within_exact {
        assert_eq!(found, exact, "{metric:?}");
    }
}

#[when("an index holding no vectors is searched")]
fn empty_searched(w: &mut Steps) -> TestResult {
    let lists = new_lists();
    let p = pred("emb");
    let reported =
        search(&lists, &vkeys(&p)?, &opts(&[])?, &[0.1f32, 0.2], 3, None).map(|found| nums(&found));
    // The caller, Dgraph's similar_to, takes the empty-index result for no neighbours.
    let neighbours = match &reported {
        Err(e) if e.is_empty_index() => Ok(Vec::new()),
        Err(e) => Err(e.to_string()),
        Ok(uids) => Ok(uids.clone()),
    }
    .context("the search finds neighbours or the empty index")?;
    w.empty = Some((reported, neighbours));
    Ok(())
}

#[then(
    "the index reports an empty-index result, which the caller treats as no neighbours, and the search returns an empty result without error"
)]
fn empty_result(w: &mut Steps) -> TestResult {
    let (reported, neighbours) = w.empty.as_ref().context("w.empty.as_ref()")?;
    assert!(
        reported.as_ref().is_err_and(HnswError::is_empty_index),
        "{reported:?}"
    );
    assert!(neighbours.is_empty());
    Ok(())
}

#[when("two predicates in two graphs each hold vectors")]
fn two_graphs(w: &mut Steps) -> TestResult {
    let lists = new_lists();
    let o = opts(&[("maxLevels", "3")])?;
    let mut rng = SimRng::new(17);
    // Graph 1 and predicate 41, graph 2 and predicate 42; their nodes numbered apart.
    for (graph, predicate, nodes) in [(1, 41, 1..=10u64), (2, 42, 101..=110)] {
        let keys = VectorKeys::new("0-emb", 0, Graph::Named(id(graph)?), id(predicate)?);
        for uid in nodes.clone() {
            let v = random_vector(&mut rng, 3);
            let _inserted: Vec<KeyValue> =
                insert_at(&lists, &keys, &o, id(uid)?, &v, &mut || draw(&mut rng))?;
        }
        let found = nums(&search(
            &lists,
            &keys,
            &o,
            &random_vector(&mut rng, 3),
            30,
            None,
        )?);
        w.apart.push((keys, nodes.collect(), found));
    }
    w.lists = Some(lists);
    Ok(())
}

#[then(
    "each predicate's HNSW lists are under its own graph and predicate, and a search of one finds none of the other's nodes"
)]
fn lists_apart(w: &mut Steps) -> TestResult {
    let lists = lists(w)?;
    let snapshot = lists.store().snapshot(lists.applied_ts())?;
    let prefix = key_prefix(Keyspace::VectorIndex, false, 0, None, None);
    let mut held: Vec<(Graph, SystemId, u64)> = Vec::new();
    for item in snapshot.scan(&Scan::prefix(prefix)) {
        let (parsed, _) = QuadKey::parse(item?.key())?;
        let QuadKey::VectorIndex {
            graph,
            predicate,
            node,
            ..
        } = parsed
        else {
            return Err(ultragraphdb_tool_bdd::TestError::new(
                "a vector-index key of another kind",
            ));
        };
        held.push((graph, predicate, node.sequence().get()));
    }
    assert_eq!(w.apart.len(), 2);
    for (keys, nodes, found) in &w.apart {
        // Every node's lists, the entry point and nothing else of the other index under this graph and predicate.
        let mine: Vec<u64> = held
            .iter()
            .filter(|(g, p, _)| (*g, *p) == (keys.graph(), keys.predicate()))
            .map(|(_, _, n)| *n)
            .collect();
        let predicate = keys.predicate().sequence().get();
        let lists_of_nodes: Vec<u64> = mine.iter().copied().filter(|n| *n != predicate).collect();
        assert_eq!(&lists_of_nodes, nodes, "{keys:?}");
        assert!(mine.contains(&predicate), "the entry point of {keys:?}");
        assert_eq!(found.len(), nodes.len(), "{keys:?} finds its own nodes");
        assert!(
            found.iter().all(|n| nodes.contains(n)),
            "{keys:?}: {found:?}"
        );
    }
    assert_eq!(
        held.len(),
        2 * 11,
        "10 nodes' lists and an entry point each"
    );
    Ok(())
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
