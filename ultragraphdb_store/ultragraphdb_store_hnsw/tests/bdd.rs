/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The vector-index scenarios, run over the posting lists of the simulated store: every insert and delete in a
//! request committed on its own, as index maintenance makes them, every search through a request at the newest
//! timestamp.

use cucumber::{World, then, when};
use ultragraphdb_core_tok::{VEC_DEAD, VEC_ENTRY, VEC_KEYWORD};
use ultragraphdb_store_hnsw::{HnswError, bytes_to_uint64, decode_uint64_matrix, parse_edges};
use ultragraphdb_store_sim::SimRng;
use ultragraphdb_traits_index::{KeyValue, Metric, VectorIndexOptions, accept_all};

// The unit tests' helpers, shared by path.
mod utils {
    #[allow(dead_code)]
    #[path = "helpers.rs"]
    pub mod helpers;
}

use crate::utils::helpers::{
    TestLists, brute_force, delete, draw, index_value, insert, key, new_lists, opts, oracle_score,
    pred, search,
};

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
    empty: Option<EmptySearch>,
    heir_lists: Vec<Vec<u64>>,
}

fn lists(w: &Steps) -> &TestLists {
    w.lists.as_ref().expect("lists")
}

fn widen(v: &[f32]) -> Vec<f64> {
    v.iter().map(|&f| f64::from(f)).collect()
}

fn widened(vectors: &[(u64, Vec<f32>)]) -> Vec<(u64, Vec<f64>)> {
    vectors.iter().map(|(uid, v)| (*uid, widen(v))).collect()
}

fn random_vector(rng: &mut SimRng, dims: usize) -> Vec<f32> {
    (0..dims).map(|_| (draw(rng) * 2.0 - 1.0) as f32).collect()
}

#[when("the first vector is inserted into an empty index")]
fn first_insert(w: &mut Steps) {
    w.lists = Some(new_lists());
    w.pred = pred("emb");
    w.uid = 7;
    insert(
        lists(w),
        &w.pred,
        &opts(&[("maxLevels", "3")]),
        7,
        &[0.5f32, 0.25],
        &mut || 0.5,
    )
    .unwrap();
}

#[then("the entry point names its uid and its neighbour lists exist, empty, at every level")]
fn entry_and_empty_lists(w: &mut Steps) {
    let ts = lists(w).applied_ts();
    let entry = index_value(lists(w), &key(&format!("{}{VEC_ENTRY}", w.pred), 1), ts).unwrap();
    assert_eq!(bytes_to_uint64(&entry), Some(w.uid));
    let node = index_value(
        lists(w),
        &key(&format!("{}{VEC_KEYWORD}", w.pred), w.uid),
        ts,
    )
    .unwrap();
    assert_eq!(decode_uint64_matrix(&node), Ok(vec![Vec::new(); 3]));
}

#[when("a vector is deleted")]
fn vector_deleted(w: &mut Steps) {
    w.lists = Some(new_lists());
    w.pred = pred("emb");
    let o = opts(&[
        ("maxLevels", "3"),
        ("efConstruction", "6"),
        ("efSearch", "6"),
    ]);
    let mut rng = SimRng::new(3);
    for uid in 1..=20 {
        let v = random_vector(&mut rng, 3);
        insert(lists(w), &w.pred, &o, uid, &v, &mut || draw(&mut rng)).unwrap();
    }
    // Uid 1 is the entry point; deleting another leaves the graph rooted.
    w.uid = 9;
    delete(lists(w), &w.pred, w.uid).unwrap();
    for uid in 21..=40 {
        let v = random_vector(&mut rng, 3);
        let edges = insert(lists(w), &w.pred, &o, uid, &v, &mut || draw(&mut rng)).unwrap();
        w.later_edges.extend(edges);
    }
}

#[then(
    "its uid is added to the dead list, and later inserts leave it out of the neighbour lists they write"
)]
fn dead_and_left_out(w: &mut Steps) {
    let ts = lists(w).applied_ts();
    let dead = index_value(lists(w), &key(&format!("{}{VEC_DEAD}", w.pred), 1), ts).unwrap();
    assert_eq!(
        parse_edges(&String::from_utf8_lossy(&dead)).unwrap(),
        vec![w.uid]
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
        let levels = decode_uint64_matrix(e.value()).unwrap();
        assert!(
            !levels.iter().flatten().any(|&u| u == w.uid),
            "the lists of {} name {}",
            e.entity(),
            w.uid
        );
    }
}

#[when(
    "50 vectors are indexed under each metric and a search asks for the 5 nearest to a query vector"
)]
fn fifty_per_metric(w: &mut Steps) {
    for metric in [Metric::Euclidean, Metric::Cosine, Metric::DotProduct] {
        let lists = new_lists();
        let p = pred("emb");
        let o = opts(&[("metric", metric.name())]);
        let mut rng = SimRng::new(50);
        let mut vectors = Vec::new();
        for uid in 1..=50 {
            let v = random_vector(&mut rng, 4);
            insert(&lists, &p, &o, uid, &v, &mut || draw(&mut rng)).unwrap();
            vectors.push((uid, v));
        }
        let query = random_vector(&mut rng, 4);
        let found = search(
            &lists,
            &p,
            &o,
            &query,
            5,
            Some(&VectorIndexOptions::default()),
        )
        .unwrap();
        // The exhaustive oracle, its distances computed naively in float64, not by the index's kernels.
        let brute = brute_force(metric, &widened(&vectors), &widen(&query), 5);
        w.compared.push((metric, found, brute));
    }
}

#[then("the result equals the 5 nearest by brute force under that metric")]
fn equals_brute_force(w: &mut Steps) {
    assert_eq!(w.compared.len(), 3);
    for (metric, found, brute) in &w.compared {
        assert_eq!(found, brute, "{metric:?}");
    }
}

#[when("the entry point's vector is deleted and another vector is inserted")]
fn entry_deleted(w: &mut Steps) {
    w.lists = Some(new_lists());
    w.pred = pred("emb");
    let o = opts(&[
        ("maxLevels", "3"),
        ("efConstruction", "6"),
        ("efSearch", "6"),
    ]);
    let mut rng = SimRng::new(5);
    for uid in 1..=12 {
        let v = random_vector(&mut rng, 3);
        insert(lists(w), &w.pred, &o, uid, &v, &mut || draw(&mut rng)).unwrap();
    }
    let ts = lists(w).applied_ts();
    let heir_key = key(&format!("{}{VEC_KEYWORD}", w.pred), 2);
    w.heir_lists = decode_uint64_matrix(&index_value(lists(w), &heir_key, ts).unwrap()).unwrap();
    // Uid 1 is the entry point; uid 2 is the first node holding a vector after it.
    delete(lists(w), &w.pred, 1).unwrap();
    w.uid = 13;
    let v = random_vector(&mut rng, 3);
    insert(lists(w), &w.pred, &o, 13, &v, &mut || draw(&mut rng)).unwrap();
}

#[then(
    "the first node holding a vector becomes the entry point with its neighbour lists kept, and the new vector is inserted"
)]
fn handed_over(w: &mut Steps) {
    // Dgraph rewrites uid 2 with empty lists at every level and does not insert 13 (ledger 45).
    let ts = lists(w).applied_ts();
    let entry = index_value(lists(w), &key(&format!("{}{VEC_ENTRY}", w.pred), 1), ts).unwrap();
    assert_eq!(bytes_to_uint64(&entry), Some(2));
    let vec_key = format!("{}{VEC_KEYWORD}", w.pred);
    let heir =
        decode_uint64_matrix(&index_value(lists(w), &key(&vec_key, 2), ts).unwrap()).unwrap();
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
        decode_uint64_matrix(&index_value(lists(w), &key(&vec_key, w.uid), ts).unwrap()).unwrap();
    assert!(new.iter().any(|l| !l.is_empty()), "13 is linked: {new:?}");
}

#[when("a search carries a distance threshold")]
fn threshold_search(w: &mut Steps) {
    for (metric, threshold) in [(Metric::Euclidean, 0.7), (Metric::Cosine, 0.2)] {
        let lists = new_lists();
        let p = pred("emb");
        let o = opts(&[("metric", metric.name())]);
        let mut rng = SimRng::new(70);
        let mut vectors = Vec::new();
        for uid in 1..=40 {
            let v = random_vector(&mut rng, 3);
            insert(&lists, &p, &o, uid, &v, &mut || draw(&mut rng)).unwrap();
            vectors.push((uid, v));
        }
        let query = random_vector(&mut rng, 3);
        let options = VectorIndexOptions::new(0, Some(threshold), accept_all::<f32>);
        let found = search(&lists, &p, &o, &query, 40, Some(&options)).unwrap();
        let distances = found
            .iter()
            .map(|uid| {
                let (_, v) = vectors.iter().find(|(u, _)| u == uid).unwrap();
                let score = oracle_score(metric, &widen(v), &widen(&query));
                if metric == Metric::Cosine {
                    1.0 - score
                } else {
                    score
                }
            })
            .collect();
        w.within.push((metric, threshold, distances));
    }
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
}

#[when("an index holding no vectors is searched")]
fn empty_searched(w: &mut Steps) {
    let lists = new_lists();
    let p = pred("emb");
    let reported = search(&lists, &p, &opts(&[]), &[0.1f32, 0.2], 3, None);
    // The caller, Dgraph's similar_to, takes the empty-index result for no neighbours.
    let neighbours = match &reported {
        Err(e) if e.is_empty_index() => Vec::new(),
        Err(e) => panic!("{e}"),
        Ok(uids) => uids.clone(),
    };
    w.empty = Some((reported, neighbours));
}

#[then(
    "the index reports an empty-index result, which the caller treats as no neighbours, and the search returns an empty result without error"
)]
fn empty_result(w: &mut Steps) {
    let (reported, neighbours) = w.empty.as_ref().unwrap();
    assert!(
        reported.as_ref().is_err_and(HnswError::is_empty_index),
        "{reported:?}"
    );
    assert!(neighbours.is_empty());
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()),
    );
}
