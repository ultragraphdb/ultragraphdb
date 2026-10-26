/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The keys of one index, read back through the quad key parser: each names its namespace, graph and predicate, so
//! two predicates' or two graphs' lists never meet.

use ultragraphdb_core_keys::QuadKey;
use ultragraphdb_store_hnsw::{DEAD_LIST, ENTRY_LIST, NEIGHBOUR_LIST, VectorKeys};

fn parsed(key: &[u8]) -> QuadKey {
    let (key, split) = QuadKey::parse(key).expect("a quad key");
    assert_eq!(split, None);
    key
}

#[test]
fn test_every_key_names_its_namespace_graph_and_predicate() {
    let keys = VectorKeys::new("0-emb", 3, 5, 7);
    assert_eq!(
        (
            keys.name(),
            keys.namespace(),
            keys.graph(),
            keys.predicate()
        ),
        ("0-emb", 3, 5, 7)
    );
    assert_eq!(
        parsed(&keys.vector(42)),
        QuadKey::Forward {
            namespace: 3,
            graph: 5,
            predicate: 7,
            subject: 42
        }
    );
    for (key, list, node) in [
        (keys.neighbours(42), NEIGHBOUR_LIST, 42),
        (keys.entry(), ENTRY_LIST, 1),
        (keys.dead(), DEAD_LIST, 1),
    ] {
        assert_eq!(
            parsed(&key),
            QuadKey::VectorIndex {
                namespace: 3,
                graph: 5,
                predicate: 7,
                list,
                node
            }
        );
    }
    // The list kinds, and the labels Dgraph's attributes give the values.
    assert_eq!((ENTRY_LIST, NEIGHBOUR_LIST, DEAD_LIST), (0, 1, 2));
    assert_eq!(
        (
            keys.neighbours_label(),
            keys.entry_label(),
            keys.dead_label()
        ),
        (
            "0-emb__vector_".to_string(),
            "0-emb__vector_entry".to_string(),
            "0-emb__vector_dead".to_string()
        )
    );
}

#[test]
fn test_the_vector_prefix_holds_its_predicates_vectors_alone() {
    let keys = VectorKeys::new("0-emb", 0, 5, 7);
    let prefix = keys.vector_prefix();
    assert!(keys.vector(1).starts_with(&prefix));
    assert!(keys.vector(u64::MAX).starts_with(&prefix));
    for other in [
        VectorKeys::new("0-emb", 0, 5, 8),
        VectorKeys::new("0-emb", 0, 6, 7),
        VectorKeys::new("0-emb", 1, 5, 7),
    ] {
        assert!(!other.vector(1).starts_with(&prefix), "{other:?}");
        assert_ne!(other.neighbours(1), keys.neighbours(1), "{other:?}");
        assert_ne!(other.entry(), keys.entry(), "{other:?}");
    }
    assert!(!keys.neighbours(1).starts_with(&prefix));
    assert_ne!(keys.entry(), keys.dead());
    assert_ne!(
        keys.neighbours(1),
        keys.entry(),
        "node 1's lists are not the entry point"
    );
}

#[test]
fn test_node_of_reads_back_its_own_forward_lists_only() {
    let keys = VectorKeys::new("0-emb", 2, 5, 7);
    assert_eq!(keys.node_of(&keys.vector(42)), Some(42));
    assert_eq!(keys.node_of(&keys.neighbours(42)), None, "a neighbour list");
    assert_eq!(keys.node_of(&keys.entry()), None);
    for other in [
        VectorKeys::new("0-emb", 2, 5, 8),
        VectorKeys::new("0-emb", 2, 6, 7),
        VectorKeys::new("0-emb", 3, 5, 7),
    ] {
        assert_eq!(keys.node_of(&other.vector(42)), None, "{other:?}");
    }
    assert_eq!(keys.node_of(b""), None, "no key");
    assert_eq!(keys.node_of(&keys.vector_prefix()), None, "a prefix");
}
