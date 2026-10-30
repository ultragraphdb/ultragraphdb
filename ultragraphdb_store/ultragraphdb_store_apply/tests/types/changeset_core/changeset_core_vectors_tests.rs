/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The vector indexer and the search: committed vectors reach the HNSW lists only through the indexer, in commit
//! order, which records the version it indexed through; a search returns only nodes the lists hold whose vector is
//! visible at its snapshot.

use oxrdf::{GraphNameRef, NamedNode};
use ultragraphdb_core_keys::{DEFAULT_GRAPH, Keyspace};
use ultragraphdb_store_apply::{ChangesetErrorEnum, LevelDraw};
use ultragraphdb_store_hnsw::{
    VectorKeys, bytes_to_uint64, decode_uint64_matrix, insert_layer, parse_edges,
};
use ultragraphdb_traits_index::{VectorIndexOptions, accept_all};

use crate::utils::changesets::{
    NS, TestChangesets, apply, changesets, declare, delete, ex, finish_builds, index, insert,
    keys_of, uid, write,
};

/// The keys of `emb`'s vector index in the default graph.
fn keys(a: &TestChangesets) -> VectorKeys {
    VectorKeys::new(ex("emb").as_str(), NS, DEFAULT_GRAPH, uid(a, "emb"))
}

/// The single value of the vector-index list `key` at the newest version.
fn value(a: &TestChangesets, key: &[u8]) -> Option<Vec<u8>> {
    let latest = a.quads().lists().applied_ts();
    let list = a.quads().lists().read(key, latest).unwrap();
    list.static_value(latest)
        .and_then(|pl| pl.postings().first())
        .map(|p| p.value().to_vec())
}

/// The nodes `emb`'s dead list holds.
fn dead(a: &TestChangesets) -> Vec<u64> {
    value(a, &keys(a).dead())
        .map(|v| parse_edges(&String::from_utf8_lossy(&v)).unwrap())
        .unwrap_or_default()
}

/// The `k` nodes of `emb` nearest `query` in the default graph at `read_ts`.
fn search_at(a: &TestChangesets, query: &[f32], k: usize, read_ts: u64) -> Vec<u64> {
    a.search_vectors(
        NS,
        GraphNameRef::DefaultGraph,
        ex("emb").as_ref(),
        query,
        k,
        &VectorIndexOptions::default(),
        read_ts,
    )
    .unwrap()
}

/// The `k` nodes of `emb` nearest `query` at the newest version.
fn search(a: &TestChangesets, query: &[f32], k: usize) -> Vec<u64> {
    search_at(a, query, k, a.quads().lists().applied_ts())
}

fn emb_with(a: &TestChangesets, texts: &[(&str, &str)]) {
    let lines: Vec<String> = texts
        .iter()
        .map(|(s, v)| format!("<{s}> <emb> \"{v}\"^^ugdb:float32vector"))
        .collect();
    write(a, &lines);
}

#[test]
fn test_committed_vectors_reach_the_lists_and_searches_only_through_the_indexer() {
    let a = changesets();
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 2",
    );
    emb_with(&a, &[("a", "[1, 0]"), ("b", "[0, 1]")]);
    assert!(keys_of(&a, Keyspace::VectorIndex, "emb").is_empty());
    // Committed and not indexed: absent from a search, though the forward lists hold both.
    assert_eq!(search(&a, &[1.0, 0.0], 2), Vec::<u64>::new());
    assert_eq!(a.indexed_version(NS, ex("emb").as_ref()).unwrap(), 0);
    let watermark = a.quads().lists().applied_ts();
    let indexed = index(&a, "emb");
    assert_eq!(indexed, watermark);
    assert_eq!(a.indexed_version(NS, ex("emb").as_ref()).unwrap(), indexed);
    // The pass wrote the lists at a version of its own, above every version it indexed.
    assert!(a.quads().lists().applied_ts() > watermark);
    assert!(!keys_of(&a, Keyspace::VectorIndex, "emb").is_empty());
    assert_eq!(search(&a, &[1.0, 0.0], 1), vec![uid(&a, "a")]);
    assert_eq!(search(&a, &[0.0, 1.0], 1), vec![uid(&a, "b")]);
}

#[test]
fn test_a_pass_with_nothing_to_index_takes_no_version() {
    let a = changesets();
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 2",
    );
    emb_with(&a, &[("a", "[1, 0]")]);
    index(&a, "emb");
    let after = a.quads().lists().applied_ts();
    // Nothing committed since: the pass reads at the watermark, writes nothing and records it.
    assert_eq!(index(&a, "emb"), after);
    assert_eq!(a.quads().lists().applied_ts(), after);
    // A round without vectors moves the watermark; the next pass records it, writing nothing.
    write(&a, &["<x> <name> \"X\""]);
    let moved = a.quads().lists().applied_ts();
    assert_eq!(index(&a, "emb"), moved);
    assert_eq!(a.quads().lists().applied_ts(), moved);
    assert_eq!(a.indexed_version(NS, ex("emb").as_ref()).unwrap(), moved);
}

#[test]
fn test_the_indexer_inserts_in_commit_order_not_in_uid_order() {
    let a = changesets();
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 2",
    );
    // `a` takes its UID first, `b` commits its vector first.
    write(&a, &["<a> <name> \"A\"", "<b> <name> \"B\""]);
    assert!(uid(&a, "a") < uid(&a, "b"));
    emb_with(&a, &[("b", "[0, 1]"), ("a", "[1, 0]")]);
    index(&a, "emb");
    // The first vector inserted into the empty index is its entry point.
    let entry = value(&a, &keys(&a).entry()).expect("an entry point");
    assert_eq!(bytes_to_uint64(&entry), Some(uid(&a, "b")));
    // A later pass keeps an entry point that holds its vector, though `a` comes first in key order.
    emb_with(&a, &[("c", "[1, 1]")]);
    index(&a, "emb");
    let entry = value(&a, &keys(&a).entry()).expect("an entry point");
    assert_eq!(bytes_to_uint64(&entry), Some(uid(&a, "b")));
}

#[test]
fn test_a_vector_deleted_is_marked_dead_by_the_indexer_and_set_again_leaves_the_list() {
    let a = changesets();
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 2",
    );
    emb_with(&a, &[("a", "[1, 0]"), ("b", "[0, 1]"), ("c", "[1, 1]")]);
    index(&a, "emb");
    apply(&a, vec![delete("<a> <emb> \"[1, 0]\"^^ugdb:float32vector")]).unwrap();
    // The transaction wrote its forward list alone.
    assert_eq!(dead(&a), Vec::<u64>::new());
    index(&a, "emb");
    assert_eq!(dead(&a), vec![uid(&a, "a")]);
    // Deleting a node already dead lists it once.
    emb_with(&a, &[("a", "[1, 0]")]);
    apply(&a, vec![delete("<a> <emb> \"[1, 0]\"^^ugdb:float32vector")]).unwrap();
    assert_eq!(dead(&a), vec![uid(&a, "a")]);
    index(&a, "emb");
    assert_eq!(dead(&a), vec![uid(&a, "a")]);
    // Set again, then indexed: out of the dead list, and found.
    emb_with(&a, &[("a", "[1, 0]")]);
    index(&a, "emb");
    assert_eq!(dead(&a), Vec::<u64>::new());
    assert_eq!(search(&a, &[1.0, 0.0], 1), vec![uid(&a, "a")]);
}

#[test]
fn test_a_vector_set_and_deleted_before_a_pass_ends_dead_and_one_deleted_and_set_ends_live() {
    let a = changesets();
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 2",
    );
    emb_with(&a, &[("a", "[1, 0]"), ("b", "[0, 1]")]);
    index(&a, "emb");
    // `c` set then deleted, `a` deleted then set: the pass reads each node's vector at its watermark.
    emb_with(&a, &[("c", "[1, 1]")]);
    apply(&a, vec![delete("<c> <emb> \"[1, 1]\"^^ugdb:float32vector")]).unwrap();
    apply(&a, vec![delete("<a> <emb> \"[1, 0]\"^^ugdb:float32vector")]).unwrap();
    emb_with(&a, &[("a", "[1, 0]")]);
    index(&a, "emb");
    assert_eq!(dead(&a), vec![uid(&a, "c")]);
    let mut found = search(&a, &[1.0, 1.0], 3);
    found.sort();
    assert_eq!(found, vec![uid(&a, "a"), uid(&a, "b")]);
}

#[test]
fn test_each_graph_and_predicate_is_indexed_on_its_own() {
    let a = changesets();
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 2",
    );
    declare(&a, "e2", "single-valued with a vector index of dimension 2");
    write(
        &a,
        &[
            "<a> <emb> \"[1, 0]\"^^ugdb:float32vector <g1>",
            "<b> <emb> \"[0, 1]\"^^ugdb:float32vector",
            "<c> <e2> \"[1, 0]\"^^ugdb:float32vector",
        ],
    );
    index(&a, "emb");
    // `e2` waits for its own pass.
    assert!(keys_of(&a, Keyspace::VectorIndex, "e2").is_empty());
    let in_g1 = a
        .search_vectors(
            NS,
            GraphNameRef::NamedNode(ex("g1").as_ref()),
            ex("emb").as_ref(),
            &[0.0, 1.0],
            3,
            &VectorIndexOptions::default(),
            a.quads().lists().applied_ts(),
        )
        .unwrap();
    assert_eq!(in_g1, vec![uid(&a, "a")]);
    assert_eq!(search(&a, &[1.0, 0.0], 3), vec![uid(&a, "b")]);
    index(&a, "e2");
    assert!(!keys_of(&a, Keyspace::VectorIndex, "e2").is_empty());
}

#[test]
fn test_the_indexer_and_the_search_refuse_a_predicate_without_a_vector_index() {
    let a = changesets();
    declare(&a, "name", "with `exact`");
    for predicate in [ex("name"), ex("unknown")] {
        let refused = a.index_vectors(NS, predicate.as_ref()).unwrap_err();
        assert!(
            matches!(&refused.0, ChangesetErrorEnum::NoVectorIndex { predicate: p } if p == predicate.as_str()),
            "{refused:?}"
        );
        let refused = a
            .search_vectors(
                NS,
                GraphNameRef::DefaultGraph,
                predicate.as_ref(),
                &[1.0],
                1,
                &VectorIndexOptions::default(),
                a.quads().lists().applied_ts(),
            )
            .unwrap_err();
        assert!(
            matches!(&refused.0, ChangesetErrorEnum::NoVectorIndex { .. }),
            "{refused:?}"
        );
    }
    assert_eq!(
        a.index_vectors(NS, ex("name").as_ref())
            .unwrap_err()
            .to_string(),
        "Predicate <http://ex.org/name> has no vector index"
    );
}

#[test]
fn test_the_indexer_leaves_a_predicate_being_built_to_its_build() {
    let a = changesets();
    declare(&a, "emb", "single-valued");
    emb_with(&a, &[("a", "[1, 0]"), ("b", "[0, 1]")]);
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 2",
    );
    assert!(a.rebuilding());
    emb_with(&a, &[("c", "[1, 1]")]);
    let before = a.quads().lists().applied_ts();
    index(&a, "emb");
    // The build's swap writes the lists: the pass wrote nothing.
    assert_eq!(a.quads().lists().applied_ts(), before);
    assert!(keys_of(&a, Keyspace::VectorIndex, "emb").is_empty());
    finish_builds(&a);
    index(&a, "emb");
    for (subject, query) in [("a", [1.0, 0.0]), ("b", [0.0, 1.0]), ("c", [1.0, 1.0])] {
        assert_eq!(search(&a, &query, 1), vec![uid(&a, subject)], "{subject}");
    }
}

#[test]
fn test_a_search_reads_its_snapshot() {
    let a = changesets();
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 2",
    );
    emb_with(&a, &[("a", "[1, 0]")]);
    let first = index(&a, "emb");
    let old = a.quads().lists().applied_ts();
    emb_with(&a, &[("b", "[0, 1]")]);
    index(&a, "emb");
    assert!(first < old);
    assert_eq!(search_at(&a, &[0.0, 1.0], 2, old), vec![uid(&a, "a")]);
    assert_eq!(search(&a, &[0.0, 1.0], 2), vec![uid(&a, "b"), uid(&a, "a")]);
}

#[test]
fn test_a_vector_deleted_and_not_yet_indexed_is_not_returned() {
    let a = changesets();
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 2",
    );
    emb_with(&a, &[("a", "[1, 0]"), ("b", "[0, 1]"), ("c", "[1, 1]")]);
    index(&a, "emb");
    // `a` is the entry point; `c`, the nearest to the query, is an ordinary node.
    for (gone, text) in [("c", "[1, 1]"), ("a", "[1, 0]")] {
        apply(
            &a,
            vec![delete(&format!(
                "<{gone}> <emb> \"{text}\"^^ugdb:float32vector"
            ))],
        )
        .unwrap();
        let found = search(&a, &[0.9, 0.9], 3);
        assert!(!found.contains(&uid(&a, gone)), "{gone}: {found:?}");
        assert!(found.contains(&uid(&a, "b")), "{gone}: {found:?}");
    }
}

#[test]
fn test_a_search_finds_nothing_in_an_empty_index_or_an_unknown_graph() {
    let a = changesets();
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 2",
    );
    // Declared, written to by nothing: the search of an empty index finds nothing, without error.
    write(&a, &["<x> <name> \"X\""]);
    assert_eq!(search(&a, &[1.0, 0.0], 3), Vec::<u64>::new());
    emb_with(&a, &[("a", "[1, 0]")]);
    index(&a, "emb");
    let unknown = NamedNode::new_unchecked("http://ex.org/nowhere");
    let found = a
        .search_vectors(
            NS,
            GraphNameRef::NamedNode(unknown.as_ref()),
            ex("emb").as_ref(),
            &[1.0, 0.0],
            3,
            &VectorIndexOptions::default(),
            a.quads().lists().applied_ts(),
        )
        .unwrap();
    assert_eq!(found, Vec::<u64>::new());
}

#[test]
fn test_a_search_honours_its_count_and_options() {
    let a = changesets();
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 2",
    );
    emb_with(
        &a,
        &[
            ("a", "[1, 0]"),
            ("b", "[0, 1]"),
            ("c", "[1, 1]"),
            ("d", "[3, 4]"),
        ],
    );
    index(&a, "emb");
    let latest = a.quads().lists().applied_ts();
    let found = |k: usize, options: &VectorIndexOptions<f32>| {
        a.search_vectors(
            NS,
            GraphNameRef::DefaultGraph,
            ex("emb").as_ref(),
            &[0.0, 0.0],
            k,
            options,
            latest,
        )
        .unwrap()
    };
    // Euclidean distances from the origin, by hand: a and b 1, c √2, d 5.
    assert_eq!(found(4, &VectorIndexOptions::default()).len(), 4);
    assert_eq!(found(4, &VectorIndexOptions::default())[3], uid(&a, "d"));
    assert_eq!(found(2, &VectorIndexOptions::default()).len(), 2);
    let within = VectorIndexOptions::new(0, Some(1.5), accept_all::<f32>);
    let mut near = found(4, &within);
    near.sort();
    assert_eq!(near, vec![uid(&a, "a"), uid(&a, "b"), uid(&a, "c")]);
}

#[test]
fn test_a_changeset_inserting_a_vector_and_another_triple_feeds_the_indexer_once() {
    let a = changesets();
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 2",
    );
    apply(
        &a,
        vec![
            insert("<a> <name> \"A\""),
            insert("<a> <emb> \"[1, 0]\"^^ugdb:float32vector"),
            insert("<b> <emb> \"[0, 1]\"^^ugdb:float32vector"),
        ],
    )
    .unwrap();
    index(&a, "emb");
    let mut found = search(&a, &[1.0, 1.0], 3);
    found.sort();
    assert_eq!(found, vec![uid(&a, "a"), uid(&a, "b")]);
}

/// The level each of `nodes` was inserted from in `emb`'s index: the first level at which its neighbour lists hold a
/// node, which for any node but the entry point is the level it drew.
fn levels(a: &TestChangesets, nodes: &[&str]) -> Vec<usize> {
    nodes
        .iter()
        .map(|n| {
            let lists = value(a, &keys(a).neighbours(uid(a, n)))
                .map(|v| decode_uint64_matrix(&v).unwrap())
                .unwrap_or_default();
            lists
                .iter()
                .position(|l| !l.is_empty())
                .expect("a node with neighbours")
        })
        .collect()
}

/// Ten nodes named in order, their vectors of dimension 3, `a` the first.
const TEN: [&str; 10] = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"];

fn ten_vectors(a: &TestChangesets) {
    let named: Vec<String> = TEN
        .iter()
        .map(|n| format!("<{n}> <name> \"{n}\""))
        .collect();
    write(a, &named);
    let vectors: Vec<String> = TEN
        .iter()
        .enumerate()
        .map(|(i, n)| {
            format!(
                "<{n}> <emb> \"[{}, {}, 1]\"^^ugdb:float32vector",
                i % 4,
                i / 4
            )
        })
        .collect();
    write(a, &vectors);
}

#[test]
fn test_the_indexer_draws_each_node_s_level_from_the_node_s_own_stream() {
    let a = changesets();
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 3",
    );
    ten_vectors(&a);
    index(&a, "emb");
    // The applier's seed is 1; the default index has 3 levels. `a`, the entry point, draws none.
    let expected: Vec<usize> = TEN[1..]
        .iter()
        .map(|n| {
            insert_layer(
                3,
                LevelDraw::of_node(1, NS, uid(&a, "emb"), uid(&a, n)).next_f64(),
            )
        })
        .collect();
    assert_eq!(levels(&a, &TEN[1..]), expected);
    assert!(expected.iter().any(|l| *l < 2), "{expected:?}");
}

#[test]
fn test_a_build_and_the_indexer_draw_each_node_the_same_level() {
    let indexed = changesets();
    declare(
        &indexed,
        "emb",
        "single-valued with a vector index of dimension 3",
    );
    ten_vectors(&indexed);
    index(&indexed, "emb");
    let built = changesets();
    declare(&built, "emb", "single-valued");
    ten_vectors(&built);
    declare(
        &built,
        "emb",
        "single-valued with a vector index of dimension 3",
    );
    finish_builds(&built);
    assert!(!built.rebuilding());
    // The build inserts in key order, the indexer in commit order: here the same, `a` first in both.
    assert_eq!(levels(&built, &TEN[1..]), levels(&indexed, &TEN[1..]));
}

#[test]
fn test_a_vector_written_before_its_predicate_declares_a_vector_index_is_not_queued() {
    let a = changesets();
    // `emb` without a vector index holds a vector, then none: no data, so the index is declared at once.
    emb_with(&a, &[("a", "[1, 0]")]);
    apply(&a, vec![delete("<a> <emb> \"[1, 0]\"^^ugdb:float32vector")]).unwrap();
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 2",
    );
    assert!(!a.rebuilding());
    emb_with(&a, &[("b", "[0, 1]")]);
    index(&a, "emb");
    // Only `b` reached the indexer: `a`'s vector came and went before its predicate had an index.
    assert_eq!(dead(&a), Vec::<u64>::new());
    assert_eq!(search(&a, &[0.0, 1.0], 2), vec![uid(&a, "b")]);
}

#[test]
fn test_the_queue_of_a_vector_index_removed_goes_with_it() {
    let a = changesets();
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 2",
    );
    emb_with(&a, &[("a", "[1, 0]")]);
    // The index is removed before a pass: the pass is refused, and `a`'s queued vector dropped.
    declare(&a, "emb", "single-valued");
    assert!(a.index_vectors(NS, ex("emb").as_ref()).is_err());
    apply(&a, vec![delete("<a> <emb> \"[1, 0]\"^^ugdb:float32vector")]).unwrap();
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 2",
    );
    assert!(!a.rebuilding());
    emb_with(&a, &[("b", "[0, 1]")]);
    index(&a, "emb");
    assert_eq!(dead(&a), Vec::<u64>::new());
}

/// Every node's neighbour lists in `emb`'s index, by subject.
fn graph_of(a: &TestChangesets, nodes: &[&str]) -> Vec<Option<Vec<u8>>> {
    nodes
        .iter()
        .map(|n| value(a, &keys(a).neighbours(uid(a, n))))
        .collect()
}

#[test]
fn test_a_node_changed_twice_before_a_pass_is_inserted_once() {
    let nodes = ["a", "b", "c", "d", "e"];
    let run = |twice: bool| {
        let a = changesets();
        declare(
            &a,
            "emb",
            "single-valued with a vector index of dimension 2",
        );
        let named: Vec<String> = nodes
            .iter()
            .map(|n| format!("<{n}> <name> \"{n}\""))
            .collect();
        write(&a, &named);
        emb_with(&a, &[("a", "[0, 0]"), ("b", "[1, 0]")]);
        if twice {
            // `c` changes, `d` after it, then `c` again: its two changes are not adjacent in commit order.
            emb_with(&a, &[("c", "[9, 9]"), ("d", "[1, 1]"), ("c", "[0, 1]")]);
        } else {
            emb_with(&a, &[("c", "[0, 1]"), ("d", "[1, 1]")]);
        }
        emb_with(&a, &[("e", "[2, 1]")]);
        index(&a, "emb");
        graph_of(&a, &nodes)
    };
    // `c` first changes at the same place in commit order in both runs, and holds the same vector at the pass.
    assert_eq!(run(true), run(false));
}

#[test]
fn test_a_dead_entry_point_hands_over_to_a_node_the_index_holds() {
    let a = changesets();
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 2",
    );
    // `p` takes the smallest UID of the nodes; its vector comes last.
    write(&a, &["<p> <name> \"P\""]);
    emb_with(&a, &[("a", "[1, 0]"), ("b", "[0, 1]"), ("c", "[1, 1]")]);
    index(&a, "emb");
    assert_eq!(
        bytes_to_uint64(&value(&a, &keys(&a).entry()).unwrap()),
        Some(uid(&a, "a"))
    );
    // The entry point's vector is deleted and `p`, not yet indexed and first in key order, written in one pass.
    apply(&a, vec![delete("<a> <emb> \"[1, 0]\"^^ugdb:float32vector")]).unwrap();
    emb_with(&a, &[("p", "[5, 5]")]);
    index(&a, "emb");
    // The entry point is the first node in key order the index held with a vector, `b`, and every node is reachable.
    assert_eq!(
        bytes_to_uint64(&value(&a, &keys(&a).entry()).unwrap()),
        Some(uid(&a, "b"))
    );
    for (subject, query) in [("b", [0.0, 1.0]), ("c", [1.0, 1.0]), ("p", [5.0, 5.0])] {
        assert_eq!(search(&a, &query, 1), vec![uid(&a, subject)], "{subject}");
    }
}

#[test]
fn test_an_index_whose_every_node_lost_its_vector_starts_afresh_from_the_first_insert() {
    let a = changesets();
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 2",
    );
    write(&a, &["<p> <name> \"P\""]);
    emb_with(&a, &[("a", "[1, 0]"), ("b", "[0, 1]")]);
    index(&a, "emb");
    for (gone, text) in [("a", "[1, 0]"), ("b", "[0, 1]")] {
        apply(
            &a,
            vec![delete(&format!(
                "<{gone}> <emb> \"{text}\"^^ugdb:float32vector"
            ))],
        )
        .unwrap();
    }
    // `c` commits first, `p` takes the smallest UID: no node the index holds has a vector, so `c` starts it again.
    emb_with(&a, &[("c", "[1, 1]"), ("d", "[2, 1]"), ("p", "[5, 5]")]);
    index(&a, "emb");
    assert_eq!(
        bytes_to_uint64(&value(&a, &keys(&a).entry()).unwrap()),
        Some(uid(&a, "c"))
    );
    for (subject, query) in [("c", [1.0, 1.0]), ("d", [2.0, 1.0]), ("p", [5.0, 5.0])] {
        assert_eq!(search(&a, &query, 1), vec![uid(&a, subject)], "{subject}");
    }
}
