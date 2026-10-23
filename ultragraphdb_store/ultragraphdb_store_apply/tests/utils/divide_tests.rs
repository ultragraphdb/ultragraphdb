/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/x_test.go (TestDivideAndRule). Changed for ultragraphdb: translated to Rust;
 * added: the batches applyMutations cuts, and the port's coarsening to whole predicates.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_proto_pb::pb::DirectedEdge;
use ultragraphdb_store_apply::{divide_and_rule, edge_batches};

#[test]
fn test_divide_and_rule() {
    for (num, go, width) in [
        (68, 1, 68),
        (255, 1, 255),
        (256, 1, 256),
        (510, 1, 510),
        (511, 2, 256),
        (512, 2, 256),
        (513, 2, 257),
        (768, 2, 384),
        (1755, 4, 439),
    ] {
        assert_eq!(divide_and_rule(num), (go, width), "{num}");
    }
}

fn edge(attr: &str, entity: u64) -> DirectedEdge {
    DirectedEdge {
        attr: attr.into(),
        entity,
        ..DirectedEdge::default()
    }
}

/// Dgraph's batches of sorted edges, `width` long and never splitting one subject's edges of a predicate, coarsened
/// so that no predicate spans two batches: each predicate's keys are its own, so batches touch disjoint lists.
#[test]
fn test_edge_batches_keep_each_predicate_whole() {
    let mut edges: Vec<DirectedEdge> = (0..700).map(|i| edge("0-a", i / 3)).collect();
    edges.extend((0..400).map(|i| edge("0-b", i)));
    edges.extend((0..10).map(|i| edge("0-c", i)));
    let batches = edge_batches(&edges);
    assert_eq!(batches, vec![0..700, 700..1100, 1100..1110]);
    let few: Vec<DirectedEdge> = (0..5).map(|i| edge("0-a", i)).collect();
    assert_eq!(edge_batches(&few), vec![0..5]);
    assert!(edge_batches(&[]).is_empty());
}
