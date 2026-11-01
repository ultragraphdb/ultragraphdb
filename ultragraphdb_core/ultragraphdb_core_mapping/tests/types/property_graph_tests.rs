/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{NamedNode, NamedOrBlankNode};
use ultragraphdb_core_mapping::{PgEdge, PgNode, PropertyGraph, StatementId};

fn node(name: &str) -> NamedOrBlankNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}")).into()
}

#[test]
fn test_edges_are_kept_ordered_by_source_whatever_order_they_are_added_in() {
    // `<http://ex.org/a>` sorts before `<http://ex.org/b>` as text, so the edge from a comes first.
    let pg = PropertyGraph::new()
        .with_edge(PgEdge::new(node("b"), "knows", node("c")))
        .with_edge(PgEdge::new(node("a"), "knows", node("c")));
    assert_eq!(
        pg.edges(),
        [
            PgEdge::new(node("a"), "knows", node("c")),
            PgEdge::new(node("b"), "knows", node("c")),
        ]
    );
}

#[test]
fn test_removing_edge_ids_keeps_the_nodes_and_the_edges_without_their_ids() {
    let pg = PropertyGraph::new()
        .with_node(PgNode::new(node("a")).with_label("Person"))
        .with_edge(PgEdge::new(node("b"), "knows", node("a")).with_id(StatementId::new(1)))
        .with_edge(PgEdge::new(node("a"), "knows", node("b")).with_id(StatementId::new(2)));
    let stripped = pg.without_edge_ids();
    assert_eq!(
        stripped.node("<http://ex.org/a>"),
        Some(&PgNode::new(node("a")).with_label("Person"))
    );
    assert_eq!(
        stripped.edges(),
        [
            PgEdge::new(node("a"), "knows", node("b")),
            PgEdge::new(node("b"), "knows", node("a")),
        ]
    );
}
