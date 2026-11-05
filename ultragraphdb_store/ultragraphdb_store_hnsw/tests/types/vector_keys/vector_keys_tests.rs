/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The keys of one index, read back through the quad key parser: each names its tenant, graph and predicate, so
//! two predicates' or two graphs' lists never meet.

use ultragraphdb_core_keys::{Graph, QuadKey};
use ultragraphdb_store_hnsw::{DEAD_LIST, ENTRY_LIST, NEIGHBOUR_LIST, VectorKeys};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::helpers::id;

fn parsed(key: &[u8]) -> TestResult<QuadKey> {
    let (key, split) = QuadKey::parse(key).context("a quad key")?;
    assert_eq!(split, None);
    Ok(key)
}

/// The keys of the index `0-emb` of the predicate numbered `predicate` in the graph numbered `graph` of `tenant`.
fn keys_of(tenant: u64, graph: u64, predicate: u64) -> TestResult<VectorKeys> {
    Ok(VectorKeys::new(
        "0-emb",
        tenant,
        Graph::Named(id(graph)?),
        id(predicate)?,
    ))
}

#[test]
fn test_every_key_names_its_tenant_graph_and_predicate() -> TestResult {
    let keys = keys_of(3, 5, 7)?;
    let (graph, predicate) = (Graph::Named(id(5)?), id(7)?);
    assert_eq!(
        (keys.name(), keys.tenant(), keys.graph(), keys.predicate()),
        ("0-emb", 3, graph, predicate)
    );
    assert_eq!(
        parsed(&keys.vector(id(42)?))?,
        QuadKey::Forward {
            tenant: 3,
            graph,
            predicate,
            subject: id(42)?
        }
    );
    // The entry point and the dead list are under the predicate's own ID, their list kinds apart from its lists.
    for (key, list, node) in [
        (keys.neighbours(id(42)?), NEIGHBOUR_LIST, id(42)?),
        (keys.entry(), ENTRY_LIST, predicate),
        (keys.dead(), DEAD_LIST, predicate),
    ] {
        assert_eq!(
            parsed(&key)?,
            QuadKey::VectorIndex {
                tenant: 3,
                graph,
                predicate,
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
    Ok(())
}

#[test]
fn test_the_entry_key_has_its_hand_written_bytes() -> TestResult {
    // The vector-index keyspace 0x16, tenant 3 big-endian, the graph and the predicate as IDs (type 1, sequence
    // 5 and 7, bucket 0), list kind 0, the node the predicate's ID again.
    let id_bytes = |sequence: u8| [0, 1, 0, 0, 0, 0, 0, 0, 0, sequence, 0, 0, 0, 0, 0, 0];
    let mut expected = vec![0x16, 0, 0, 0, 0, 0, 0, 0, 3];
    expected.extend_from_slice(&id_bytes(5));
    expected.extend_from_slice(&id_bytes(7));
    expected.push(0);
    expected.extend_from_slice(&id_bytes(7));
    assert_eq!(keys_of(3, 5, 7)?.entry(), expected);
    Ok(())
}

#[test]
fn test_the_vector_prefix_holds_its_predicates_vectors_alone() -> TestResult {
    let keys = keys_of(0, 5, 7)?;
    let prefix = keys.vector_prefix();
    assert!(keys.vector(id(1)?).starts_with(&prefix));
    assert!(keys.vector(id(u64::MAX)?).starts_with(&prefix));
    let default_graph = VectorKeys::new("0-emb", 0, Graph::Default, id(7)?);
    for other in [
        keys_of(0, 5, 8)?,
        keys_of(0, 6, 7)?,
        keys_of(1, 5, 7)?,
        default_graph,
    ] {
        assert!(!other.vector(id(1)?).starts_with(&prefix), "{other:?}");
        assert_ne!(
            other.neighbours(id(1)?),
            keys.neighbours(id(1)?),
            "{other:?}"
        );
        assert_ne!(other.entry(), keys.entry(), "{other:?}");
        assert_ne!(other.dead(), keys.dead(), "{other:?}");
    }
    assert!(!keys.neighbours(id(1)?).starts_with(&prefix));
    assert_ne!(keys.entry(), keys.dead());
    assert_ne!(
        keys.neighbours(keys.predicate()),
        keys.entry(),
        "a node numbered as the predicate keeps its lists apart from the entry point"
    );
    assert_ne!(keys.neighbours(keys.predicate()), keys.dead());
    Ok(())
}

#[test]
fn test_node_of_reads_back_its_own_forward_lists_only() -> TestResult {
    let keys = keys_of(2, 5, 7)?;
    assert_eq!(keys.node_of(&keys.vector(id(42)?)), Some(id(42)?));
    assert_eq!(
        keys.node_of(&keys.neighbours(id(42)?)),
        None,
        "a neighbour list"
    );
    assert_eq!(keys.node_of(&keys.entry()), None);
    for other in [keys_of(2, 5, 8)?, keys_of(2, 6, 7)?, keys_of(3, 5, 7)?] {
        assert_eq!(keys.node_of(&other.vector(id(42)?)), None, "{other:?}");
    }
    assert_eq!(keys.node_of(b""), None, "no key");
    assert_eq!(keys.node_of(&keys.vector_prefix()), None, "a prefix");
    Ok(())
}
