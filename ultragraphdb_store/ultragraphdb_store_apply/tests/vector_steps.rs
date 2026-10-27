/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The steps of the vector-index scenarios over a changeset applier: its HNSW lists in the vector-index keyspace, read
//! back through the index's own keys, its dead list, and searches through a request of the posting lists.

use cucumber::{then, when};
use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey};
use ultragraphdb_core_mapping::{
    Cardinality, Change, Declaration, PredicateDeclaration, VectorIndexSpec,
};
use ultragraphdb_store_hnsw::{
    PersistentHnsw, QueryCache, VectorKeys, ViTxn, bytes_to_uint64, decode_uint64_matrix,
    parse_edges,
};
use ultragraphdb_traits_index::{Options, accept_all};

use crate::Steps;
use crate::changeset_steps::{cs, is_held, last};
use crate::utils::changesets::{
    NS, TestChangesets, apply, declare, delete, ex, insert, stored_keys, uid, write,
};

/// A vector literal's text of `dimension` elements, its elements drawn from `seed`, in their canonical form.
fn vector(dimension: usize, seed: usize) -> String {
    let elements: Vec<String> = (0..dimension)
        .map(|i| ((i * 7 + seed * 13) % 11).to_string())
        .collect();
    format!("\"[{}]\"^^ugdb:float32vector", elements.join(", "))
}

/// The keys of the vector index of `predicate` in `graph`, the default graph for `""`.
fn keys(a: &TestChangesets, predicate: &str, graph: &str) -> VectorKeys {
    let graph = if graph.is_empty() {
        DEFAULT_GRAPH
    } else {
        uid(a, graph)
    };
    VectorKeys::new(ex(predicate).as_str(), NS, graph, uid(a, predicate))
}

/// The single value of the vector-index list `key` at the newest timestamp.
fn index_value(a: &TestChangesets, key: &[u8]) -> Option<Vec<u8>> {
    let latest = a.quads().lists().applied_ts();
    let list = a.quads().lists().read(key, latest).expect("the list reads");
    list.static_value(latest)
        .and_then(|pl| pl.postings().first())
        .map(|p| p.value().to_vec())
}

/// The nodes the dead list of `predicate` in the default graph holds.
fn dead(a: &TestChangesets, predicate: &str) -> Vec<u64> {
    index_value(a, &keys(a, predicate, "").dead())
        .map(|v| parse_edges(&String::from_utf8_lossy(&v)).expect("a dead list"))
        .unwrap_or_default()
}

/// Every level's neighbour list of `node`, empty when it has none.
fn neighbours(a: &TestChangesets, keys: &VectorKeys, node: u64) -> Vec<Vec<u64>> {
    index_value(a, &keys.neighbours(node))
        .map(|v| decode_uint64_matrix(&v).expect("a neighbour matrix"))
        .unwrap_or_default()
}

/// The `k` nodes the index at `keys` finds nearest `query`, the text of a vector literal.
fn search(a: &TestChangesets, keys: &VectorKeys, query: &str, k: usize) -> Vec<u64> {
    let latest = a.quads().lists().applied_ts();
    let mut request = a.quads().lists().request(latest);
    let txn = ViTxn::new(&mut request);
    let index =
        PersistentHnsw::<f32>::new(keys.clone(), &Options::new(), 32).expect("the index opens");
    let elements: Vec<f32> = query
        .trim_start_matches("\"[")
        .split("]\"")
        .next()
        .expect("a vector's elements")
        .split(", ")
        .map(|e| e.parse().expect("an element"))
        .collect();
    index
        .search(
            &QueryCache::new(&txn, latest),
            &elements,
            k,
            accept_all::<f32>,
        )
        .expect("the search runs")
}

/// Declares `predicate` single-valued with a vector index.
fn declare_vectors(a: &TestChangesets, predicate: &str) {
    declare(a, predicate, "single-valued with a vector index");
}

// HNSW lists in the vector-index keyspace.

#[when(
    "a vector index on `embedding` holds 768-element vectors and a 512-element vector is written to `embedding`"
)]
fn other_dimension(w: &mut Steps) {
    let a = cs(w);
    declare_vectors(&a, "embedding");
    write(
        &a,
        &[
            &format!("<a> <embedding> {}", vector(768, 1)),
            &format!("<b> <embedding> {}", vector(768, 2)),
        ],
    );
    w.results.push(apply(
        &a,
        vec![
            insert("<c> <name> \"Cid\""),
            insert(&format!("<c> <embedding> {}", vector(512, 3))),
        ],
    ));
}

#[then(
    "the write fails with an error naming `embedding`, 768 and 512, and nothing of its changeset is held"
)]
fn other_dimension_refused(w: &mut Steps) {
    let error = last(w).as_ref().expect_err("the write fails").to_string();
    for shown in [
        format!("<{}>", ex("embedding").as_str()),
        "768".into(),
        "512".into(),
    ] {
        assert!(error.contains(&shown), "{error} names {shown}");
    }
    let a = cs(w);
    assert!(!is_held(&a, "<c> <name> \"Cid\""));
}

#[when("the first vector is inserted into an empty index")]
fn first_insert(w: &mut Steps) {
    let a = cs(w);
    declare_vectors(&a, "emb");
    write(&a, &[&format!("<a> <emb> {}", vector(3, 1))]);
}

#[then("the entry point names its node's UID and its neighbour lists exist, empty, at every level")]
fn entry_created(w: &mut Steps) {
    let a = cs(w);
    let keys = keys(&a, "emb", "");
    let entry = index_value(&a, &keys.entry()).expect("an entry point");
    assert_eq!(bytes_to_uint64(&entry), Some(uid(&a, "a")));
    // 3 levels: Dgraph's default `maxLevels`, the default exponent 3 (`tok/hnsw/persistent_factory.go`).
    assert_eq!(
        neighbours(&a, &keys, uid(&a, "a")),
        vec![Vec::<u64>::new(); 3]
    );
}

#[when("a vector is deleted")]
fn vector_deleted(w: &mut Steps) {
    let a = cs(w);
    declare_vectors(&a, "emb");
    for (subject, seed) in [("a", 1), ("b", 2), ("c", 3)] {
        write(&a, &[&format!("<{subject}> <emb> {}", vector(3, seed))]);
    }
    apply(&a, vec![delete(&format!("<a> <emb> {}", vector(3, 1)))]).expect("the delete applies");
}

#[then(
    "its node's UID is added to the dead list, and later inserts leave it out of the neighbour lists they write"
)]
fn dead_left_out(w: &mut Steps) {
    let a = cs(w);
    let gone = uid(&a, "a");
    assert_eq!(dead(&a, "emb"), vec![gone]);
    let keys = keys(&a, "emb", "");
    let before: Vec<Vec<Vec<u64>>> = ["b", "c"]
        .iter()
        .map(|s| neighbours(&a, &keys, uid(&a, s)))
        .collect();
    write(&a, &[&format!("<d> <emb> {}", vector(3, 4))]);
    let d = neighbours(&a, &keys, uid(&a, "d"));
    assert!(d.iter().flatten().all(|n| *n != gone), "{d:?}");
    for (subject, before) in ["b", "c"].iter().zip(before) {
        let after = neighbours(&a, &keys, uid(&a, subject));
        if after != before {
            assert!(
                after.iter().flatten().all(|n| *n != gone),
                "{subject}: {after:?}"
            );
        }
    }
}

#[when("two predicates in two graphs each hold vectors")]
fn two_graphs(w: &mut Steps) {
    let a = cs(w);
    declare_vectors(&a, "e1");
    declare_vectors(&a, "e2");
    write(
        &a,
        &[
            &format!("<a> <e1> {} <g1>", vector(3, 1)),
            &format!("<b> <e1> {} <g1>", vector(3, 2)),
            &format!("<c> <e2> {} <g2>", vector(3, 1)),
            &format!("<d> <e2> {} <g2>", vector(3, 2)),
        ],
    );
}

#[then(
    "each predicate's HNSW lists are under its own graph and predicate, and a search of one finds none of the other's nodes"
)]
fn lists_kept_apart(w: &mut Steps) {
    let a = cs(w);
    let first = keys(&a, "e1", "g1");
    let second = keys(&a, "e2", "g2");
    let places: Vec<(u64, u64)> = stored_keys(&a)
        .iter()
        .filter_map(|k| match QuadKey::parse(k) {
            Ok((
                QuadKey::VectorIndex {
                    graph, predicate, ..
                },
                _,
            )) => Some((graph, predicate)),
            _ => None,
        })
        .collect();
    assert!(!places.is_empty());
    for place in &places {
        assert!(
            *place == (first.graph(), first.predicate())
                || *place == (second.graph(), second.predicate()),
            "{place:?}"
        );
    }
    let (ab, cd) = ([uid(&a, "a"), uid(&a, "b")], [uid(&a, "c"), uid(&a, "d")]);
    // Each index holds two nodes, so a search for four finds exactly those two.
    let sorted = |mut v: Vec<u64>| {
        v.sort();
        v
    };
    let found = search(&a, &first, &vector(3, 1), 4);
    assert_eq!(sorted(found), sorted(ab.to_vec()));
    let found = search(&a, &second, &vector(3, 1), 4);
    assert_eq!(sorted(found), sorted(cd.to_vec()));
}

// The dead list.

#[when("a vector of `emb`, declared single-valued with a vector index, is deleted by its value")]
fn deleted_by_value(w: &mut Steps) {
    let a = cs(w);
    declare_vectors(&a, "emb");
    write(
        &a,
        &[
            &format!("<a> <emb> {}", vector(3, 1)),
            &format!("<b> <emb> {}", vector(3, 2)),
        ],
    );
    apply(&a, vec![delete(&format!("<a> <emb> {}", vector(3, 1)))]).expect("the delete applies");
}

#[then("its node's UID is added to the dead list")]
fn added_dead(w: &mut Steps) {
    let a = cs(w);
    assert_eq!(dead(&a, "emb"), vec![uid(&a, "a")]);
}

#[when(
    "a vector of `emb`, declared single-valued with a vector index, is written over the vector it holds"
)]
fn overwritten(w: &mut Steps) {
    let a = cs(w);
    declare_vectors(&a, "emb");
    write(
        &a,
        &[
            &format!("<a> <emb> {}", vector(3, 1)),
            &format!("<b> <emb> {}", vector(3, 2)),
            &format!("<a> <emb> {}", vector(3, 5)),
        ],
    );
    w.names = vec![vector(3, 5)];
}

#[then("its node's UID is not in the dead list, and a search for the new vector finds it")]
fn overwritten_live(w: &mut Steps) {
    let a = cs(w);
    let node = uid(&a, "a");
    assert!(!dead(&a, "emb").contains(&node));
    assert_eq!(search(&a, &keys(&a, "emb", ""), &w.names[0], 1), vec![node]);
}

#[when(
    "a vector of `emb`, declared single-valued with a vector index, is deleted and then written again"
)]
fn deleted_and_rewritten(w: &mut Steps) {
    let a = cs(w);
    declare_vectors(&a, "emb");
    write(
        &a,
        &[
            &format!("<a> <emb> {}", vector(3, 1)),
            &format!("<b> <emb> {}", vector(3, 2)),
        ],
    );
    apply(&a, vec![delete(&format!("<a> <emb> {}", vector(3, 1)))]).expect("the delete applies");
    write(&a, &[&format!("<a> <emb> {}", vector(3, 1))]);
}

#[then(
    "its node's UID is not in the dead list, and later inserts keep it in their neighbour lists"
)]
fn rewritten_kept(w: &mut Steps) {
    let a = cs(w);
    let node = uid(&a, "a");
    assert!(!dead(&a, "emb").contains(&node));
    write(&a, &[&format!("<c> <emb> {}", vector(3, 3))]);
    let lists = neighbours(&a, &keys(&a, "emb", ""), uid(&a, "c"));
    assert!(lists.iter().flatten().any(|n| *n == node), "{lists:?}");
}

// Index parameters.

#[when("`emb` is declared with a vector index whose `maxLevels` is `0`")]
fn max_levels_zero(w: &mut Steps) {
    let a = cs(w);
    let declaration = PredicateDeclaration::new(ex("emb"))
        .with_cardinality(Cardinality::Single)
        .with_vector_index(VectorIndexSpec::new(
            "hnsw",
            vec![("maxLevels".to_string(), "0".to_string())],
        ));
    w.results.push(apply(
        &a,
        vec![Change::Declare(Declaration::Predicate(declaration))],
    ));
}

#[then(
    "the declaration fails with `IndexFactory: hnsw issues this error: 'the vector index option maxLevels must be at least 1, got 0'`, and the catalog does not hold `emb`"
)]
fn max_levels_refused(w: &mut Steps) {
    let error = last(w)
        .as_ref()
        .expect_err("the declaration fails")
        .to_string();
    assert_eq!(
        error,
        "IndexFactory: hnsw issues this error: 'the vector index option maxLevels must be at least 1, got 0'"
    );
    let a = cs(w);
    let held = a
        .quads()
        .uid_of(NS, ex("emb").as_str())
        .expect("the dictionary reads")
        .and_then(|p| a.catalog(NS).expect("the catalog").predicate(p).cloned());
    assert!(held.is_none());
}
