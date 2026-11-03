/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{Literal, NamedNode, NamedOrBlankNode};
use ultragraphdb_core_mapping::{PgEdge, PgNode, PropertyGraph};

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
fn test_parallel_edges_are_ordered_by_their_reifiers() {
    // Same source, type and target: `<http://ex.org/r1>` sorts before `<http://ex.org/r2>` as text, whatever order
    // the edges are added in.
    let reifier = |n: u8| NamedNode::new_unchecked(format!("http://ex.org/r{n}"));
    let pg = PropertyGraph::new()
        .with_edge(PgEdge::new(node("a"), "knows", node("b")).with_reifier(reifier(2)))
        .with_edge(PgEdge::new(node("a"), "knows", node("b")).with_reifier(reifier(1)));
    let reifiers: Vec<_> = pg.edges().iter().map(PgEdge::reifier).collect();
    assert_eq!(reifiers, [Some(&reifier(1)), Some(&reifier(2))]);
}

#[test]
fn test_removing_reifiers_reorders_parallel_edges_by_their_properties() {
    // r1 carries w = 2 and r2 carries w = 1, so reifier order and property order disagree: once the reifiers are
    // gone, the edge whose `w` is 1 comes first, as in a graph written with no reifiers in either order.
    let reifier = |n: u8| NamedNode::new_unchecked(format!("http://ex.org/r{n}"));
    let w = |v: i64| vec![Literal::from(v).into()];
    let plain = |v: i64| PgEdge::new(node("a"), "knows", node("b")).with_property("w", w(v));
    let stripped = PropertyGraph::new()
        .with_edge(plain(2).with_reifier(reifier(1)))
        .with_edge(plain(1).with_reifier(reifier(2)))
        .without_reifiers();
    let values: Vec<_> = stripped
        .edges()
        .iter()
        .map(|e| e.properties().get("w").cloned())
        .collect();
    assert_eq!(values, [Some(w(1)), Some(w(2))]);
    assert_eq!(
        stripped,
        PropertyGraph::new().with_edge(plain(2)).with_edge(plain(1))
    );
}

#[test]
fn test_removing_reifiers_keeps_the_nodes_and_the_edges_without_their_reifiers() {
    let reifier = |n: u8| NamedNode::new_unchecked(format!("http://ex.org/r{n}"));
    let pg = PropertyGraph::new()
        .with_node(PgNode::new(node("a")).with_label("Person"))
        .with_edge(PgEdge::new(node("b"), "knows", node("a")).with_reifier(reifier(1)))
        .with_edge(PgEdge::new(node("a"), "knows", node("b")).with_reifier(reifier(2)));
    let stripped = pg.without_reifiers();
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
