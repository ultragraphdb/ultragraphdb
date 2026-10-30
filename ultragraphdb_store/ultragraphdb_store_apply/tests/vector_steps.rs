/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The steps of the vector-index scenarios over a changeset applier: its HNSW lists in the vector-index keyspace, read
//! back through the index's own keys, its dead list, searches through a request of the posting lists and through the
//! applier, and the vector indexer, whose pass the steps run after each write unless a scenario says otherwise.

use std::num::NonZeroU16;
use std::sync::Arc;

use cucumber::{then, when};
use oxrdf::GraphNameRef;
use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey};
use ultragraphdb_core_mapping::{
    Cardinality, Change, Declaration, PredicateDeclaration, VectorIndexSpec,
};
use ultragraphdb_core_tok::get_index_factory;
use ultragraphdb_store_apply::{Executed, Reads};
use ultragraphdb_store_hnsw::{
    PersistentHnsw, QueryCache, VectorKeys, ViTxn, bytes_to_uint64, decode_uint64_matrix,
    parse_edges,
};
use ultragraphdb_traits_index::{Options, VectorIndexOptions, accept_all};

use crate::Steps;
use crate::changeset_steps::{cs, is_held, last};
use crate::utils::changesets::{
    NS, TestChangesets, apply, changeset, changesets, declare, declared, delete, ex, index, insert,
    stored_keys, uid, write, write_indexed,
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
    index
        .search(
            &QueryCache::new(&txn, latest),
            &elements(query),
            k,
            accept_all::<f32>,
        )
        .expect("the search runs")
}

/// Declares `predicate` single-valued with a vector index of dimension 3.
fn declare_vectors(a: &TestChangesets, predicate: &str) {
    declare(
        a,
        predicate,
        "single-valued with a vector index of dimension 3",
    );
}

/// Deletes `subject`'s vector `text` of `emb`, then runs the indexer's pass.
fn delete_indexed(a: &TestChangesets, subject: &str, text: &str) {
    apply(a, vec![delete(&format!("<{subject}> <emb> {text}"))]).expect("the delete applies");
    index(a, "emb");
}

// HNSW lists in the vector-index keyspace.

#[when(
    "a vector index on `embedding` is declared with dimension 768 and a 512-element vector is written to `embedding`"
)]
fn other_dimension(w: &mut Steps) {
    let a = cs(w);
    declare(
        &a,
        "embedding",
        "single-valued with a vector index of dimension 768",
    );
    // The index holds no vector: the declaration alone states the dimension.
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
    write_indexed(&a, "emb", &[&format!("<a> <emb> {}", vector(3, 1))]);
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
        write_indexed(
            &a,
            "emb",
            &[&format!("<{subject}> <emb> {}", vector(3, seed))],
        );
    }
    delete_indexed(&a, "a", &vector(3, 1));
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
    write_indexed(&a, "emb", &[&format!("<d> <emb> {}", vector(3, 4))]);
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
    index(&a, "e1");
    index(&a, "e2");
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
    write_indexed(
        &a,
        "emb",
        &[
            &format!("<a> <emb> {}", vector(3, 1)),
            &format!("<b> <emb> {}", vector(3, 2)),
        ],
    );
    delete_indexed(&a, "a", &vector(3, 1));
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
    write_indexed(
        &a,
        "emb",
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
    write_indexed(
        &a,
        "emb",
        &[
            &format!("<a> <emb> {}", vector(3, 1)),
            &format!("<b> <emb> {}", vector(3, 2)),
        ],
    );
    delete_indexed(&a, "a", &vector(3, 1));
    write_indexed(&a, "emb", &[&format!("<a> <emb> {}", vector(3, 1))]);
}

#[then(
    "its node's UID is not in the dead list, and later inserts keep it in their neighbour lists"
)]
fn rewritten_kept(w: &mut Steps) {
    let a = cs(w);
    let node = uid(&a, "a");
    assert!(!dead(&a, "emb").contains(&node));
    write_indexed(&a, "emb", &[&format!("<c> <emb> {}", vector(3, 3))]);
    let lists = neighbours(&a, &keys(&a, "emb", ""), uid(&a, "c"));
    assert!(lists.iter().flatten().any(|n| *n == node), "{lists:?}");
}

// Index parameters.

#[when("`emb` is declared with a vector index of dimension 4 whose `maxLevels` is `0`")]
fn max_levels_zero(w: &mut Steps) {
    let a = cs(w);
    let declaration = PredicateDeclaration::new(ex("emb"))
        .with_cardinality(Cardinality::Single)
        .with_vector_index(
            VectorIndexSpec::new("hnsw", vec![("maxLevels".to_string(), "0".to_string())])
                .with_dimension(NonZeroU16::new(4).expect("4 is not zero")),
        );
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
    assert!(declaration_of(&cs(w), "emb").is_none());
}

/// The declaration the catalog holds for `predicate`, `None` for none.
fn declaration_of(a: &TestChangesets, predicate: &str) -> Option<PredicateDeclaration> {
    a.quads()
        .uid_of(NS, ex(predicate).as_str())
        .expect("the dictionary reads")
        .and_then(|p| a.catalog(NS).expect("the catalog").predicate(p).cloned())
}

#[when("an hnsw index is declared with dimension 4 and `exponent: \"4\"`")]
fn exponent_four(w: &mut Steps) {
    let a = cs(w);
    let declaration = PredicateDeclaration::new(ex("emb"))
        .with_cardinality(Cardinality::Single)
        .with_vector_index(
            VectorIndexSpec::new("hnsw", vec![("exponent".to_string(), "4".to_string())])
                .with_dimension(NonZeroU16::new(4).expect("4 is not zero")),
        );
    apply(
        &a,
        vec![Change::Declare(Declaration::Predicate(declaration))],
    )
    .expect("the declaration applies");
    // The index as the catalog's declaration opens it, its options parsed by its factory.
    let held = declaration_of(&a, "emb").expect("the catalog holds `emb`");
    let spec = held.vector_index().expect("a vector index");
    let factory = get_index_factory(spec.factory()).expect("the hnsw factory");
    let mut options = Options::new();
    for (name, value) in spec.options() {
        let parsed = factory
            .allowed_options()
            .get_parsed_option(name, value)
            .expect("the option parses");
        options.set_opt(name.clone(), parsed);
    }
    let index = PersistentHnsw::<f32>::new(keys(&a, "emb", ""), &options, factory.float_bits())
        .expect("the index opens");
    w.counts = vec![vec![
        index.max_levels() as u32,
        index.ef_construction() as u32,
        index.ef_search() as u32,
    ]];
}

#[then("it uses 4 levels, an `efConstruction` of 200 and an `efSearch` of 120")]
fn exponent_parameters(w: &mut Steps) {
    assert_eq!(w.counts, vec![vec![4, 200, 120]]);
}

#[when("`emb` is declared with a vector index and no dimension")]
fn no_dimension(w: &mut Steps) {
    let a = cs(w);
    w.results
        .push(declared(&a, "emb", "single-valued with a vector index"));
}

#[then(
    "the declaration fails with an error naming `emb` and the missing dimension, and the catalog does not hold `emb`"
)]
fn no_dimension_refused(w: &mut Steps) {
    let error = last(w)
        .as_ref()
        .expect_err("the declaration fails")
        .to_string();
    assert!(
        error.contains(&format!("<{}>", ex("emb").as_str())),
        "{error}"
    );
    assert!(error.contains("dimension"), "{error}");
    assert!(declaration_of(&cs(w), "emb").is_none());
}

// Vectors indexed in commit order, outside the transaction.

/// Whether a transaction that read `reads` conflicts with `written`, committed after its read version, by the commit
/// step's rules, taken wider: any key it wrote inside a list read whole, a statement tested, a range scanned, the
/// catalog range or among the dictionary keys found absent. No conflict here is none by the narrower rules either.
fn conflicts(reads: &Reads, written: &Executed) -> bool {
    let writes = written.writes();
    let keys: Vec<&[u8]> = writes
        .written_keys()
        .chain(writes.entry_keys())
        .chain(writes.versionless().iter().map(|e| e.key()))
        .collect();
    let (catalog_start, catalog_end) = reads.catalog();
    let mut ranges = reads.ranges().to_vec();
    ranges.push((catalog_start, catalog_end));
    keys.iter().any(|k| {
        reads.lists().iter().any(|l| l.as_slice() == *k)
            || reads.statements().iter().any(|(l, _)| l.as_slice() == *k)
            || reads.absent().iter().any(|l| l.as_slice() == *k)
            || ranges.iter().any(|(start, end)| {
                *k >= start.as_slice() && end.as_ref().is_none_or(|e| *k < e.as_slice())
            })
    })
}

/// The search of `emb` in the default graph for the `k` nearest `query`, the text of a vector literal, at the newest
/// version, through the applier.
fn applier_search(a: &TestChangesets, query: &str, k: usize) -> Vec<u64> {
    a.search_vectors(
        NS,
        GraphNameRef::DefaultGraph,
        ex("emb").as_ref(),
        &elements(query),
        k,
        &VectorIndexOptions::default(),
        a.quads().lists().applied_ts(),
    )
    .expect("the search runs")
}

/// The elements of a vector literal's text.
fn elements(text: &str) -> Vec<f32> {
    text.trim_start_matches("\"[")
        .split("]\"")
        .next()
        .expect("a vector's elements")
        .split(", ")
        .map(|e| e.parse().expect("an element"))
        .collect()
}

#[when("eight transactions concurrently write vectors to `emb` for eight different nodes")]
fn eight_writers(w: &mut Steps) {
    let a = cs(w);
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 4",
    );
    write_indexed(&a, "emb", &[&format!("<n0> <emb> {}", vector(4, 9))]);
    // Each executes against the same snapshot, the watermark, before any commits.
    let executed: Vec<Executed> = (1..=8)
        .map(|i| {
            let text = format!("<n{i}> <emb> {}", vector(4, i));
            a.execute(&changeset(vec![insert(&text)]))
                .expect("the transaction executes")
        })
        .collect();
    let snapshot = a.quads().lists().applied_ts();
    assert!(executed.iter().all(|e| e.read_ts() == snapshot));
    w.flags = executed
        .iter()
        .enumerate()
        .flat_map(|(i, e)| {
            executed
                .iter()
                .enumerate()
                .filter(move |(j, _)| *j != i)
                .map(move |(_, other)| conflicts(e.reads(), other))
        })
        .collect();
    let first = a.quads().lists().next_version();
    let round: Vec<(u64, Executed)> = executed
        .into_iter()
        .enumerate()
        .map(|(i, e)| (first + 2 * i as u64, e))
        .collect();
    let versions: Vec<u64> = round.iter().map(|(v, _)| *v).collect();
    let applied = a.apply_round(round).expect("the round commits");
    w.counts = vec![
        applied.iter().map(|c| c.commit_ts() as u32).collect(),
        versions.iter().map(|v| *v as u32).collect(),
    ];
}

#[then(
    "all eight commit, and once the indexer has passed their versions a search finds each of them"
)]
fn eight_found(w: &mut Steps) {
    assert_eq!(
        w.flags.len(),
        56,
        "every ordered pair of the eight is judged"
    );
    assert!(
        w.flags.iter().all(|c| !c),
        "no transaction conflicts with another"
    );
    assert_eq!(w.counts[0], w.counts[1], "each commits at its version");
    let a = cs(w);
    let last = *w.counts[1].last().expect("eight versions") as u64;
    assert!(index(&a, "emb") >= last);
    assert!(a.indexed_version(NS, ex("emb").as_ref()).unwrap() >= last);
    for i in 1..=8 {
        assert_eq!(
            applier_search(&a, &vector(4, i), 1),
            vec![uid(&a, &format!("n{i}"))],
            "n{i}"
        );
    }
}

#[when("a vector is deleted and a search runs before the indexer has processed the delete")]
fn deleted_before_indexed(w: &mut Steps) {
    let a = cs(w);
    declare_vectors(&a, "emb");
    for (subject, seed) in [("a", 1), ("b", 2), ("c", 3)] {
        write_indexed(
            &a,
            "emb",
            &[&format!("<{subject}> <emb> {}", vector(3, seed))],
        );
    }
    // The query is `b`'s vector, and `b` is no entry point; its delete is committed, its node still in the lists.
    apply(&a, vec![delete(&format!("<b> <emb> {}", vector(3, 2)))]).expect("the delete applies");
    assert!(
        neighbours(&a, &keys(&a, "emb", ""), uid(&a, "b"))
            .iter()
            .any(|l| !l.is_empty())
    );
    w.counts = vec![
        applier_search(&a, &vector(3, 2), 3)
            .into_iter()
            .map(|u| u as u32)
            .collect(),
    ];
}

#[then("the search does not return its node")]
fn deleted_not_returned(w: &mut Steps) {
    let a = cs(w);
    let gone = uid(&a, "b") as u32;
    assert!(!w.counts[0].contains(&gone), "{:?}", w.counts[0]);
    // The others are found.
    assert_eq!(w.counts[0].len(), 2, "{:?}", w.counts[0]);
}

/// The level each of `nodes` was inserted from in `emb`'s index: the first level at which its neighbour lists hold a
/// node. A node takes neighbours at its level and every level below it, the bottom one last, and none above it.
fn levels(a: &TestChangesets, nodes: &[String]) -> Vec<u32> {
    let keys = keys(a, "emb", "");
    nodes
        .iter()
        .map(|n| {
            neighbours(a, &keys, uid(a, n))
                .iter()
                .position(|l| !l.is_empty())
                .expect("a node with neighbours") as u32
        })
        .collect()
}

#[when("the same ten vectors are indexed once in one order and once in the reverse order")]
fn ten_both_ways(w: &mut Steps) {
    let nodes: Vec<String> = (1..=10).map(|i| format!("n{i}")).collect();
    let mut runs = Vec::new();
    for reversed in [false, true] {
        let a = changesets();
        declare(
            &a,
            "emb",
            "single-valued with a vector index of dimension 3",
        );
        // The same UIDs in both stores: each node is named in the same order first.
        let named: Vec<String> = nodes
            .iter()
            .map(|n| format!("<{n}> <name> \"{n}\""))
            .collect();
        write(&a, &named);
        let mut order: Vec<usize> = (0..nodes.len()).collect();
        if reversed {
            order.reverse();
        }
        for i in order {
            write(&a, &[&format!("<{}> <emb> {}", nodes[i], vector(3, i + 1))]);
        }
        index(&a, "emb");
        runs.push(Arc::new(a));
    }
    // The entry point of each run, its first node, draws no level: compare the eight nodes that are neither.
    let inner = &nodes[1..9];
    w.counts = runs.iter().map(|a| levels(a, inner)).collect();
    w.copies = runs;
}

#[then("each node is drawn the same level both times")]
fn same_levels(w: &mut Steps) {
    assert_eq!(w.counts.len(), 2);
    assert_eq!(w.counts[0], w.counts[1]);
    // The draws place a node above the bottom level, 2 of 3: the levels compared are not all one.
    assert!(w.counts[0].iter().any(|l| *l < 2), "{:?}", w.counts[0]);
    // The two runs inserted in two orders: their graphs differ.
    let first_entry: Vec<_> = w
        .copies
        .iter()
        .map(|a| {
            let keys = keys(a, "emb", "");
            bytes_to_uint64(&index_value(a, &keys.entry()).expect("an entry point"))
        })
        .collect();
    assert_ne!(first_entry[0], first_entry[1]);
}
