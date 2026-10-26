/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::vocab::rdf;
use oxrdf::{BlankNode, GraphName, Literal, NamedNode, NamedOrBlankNode, Term};
use ultragraphdb_core_mapping::{
    CanonicalDataset, Cardinality, Change, Changeset, Names, PgValue, property_graph_view,
};

fn iri(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

fn view(
    changes: Vec<Change>,
    graph: &GraphName,
    cardinality: fn(&NamedNode) -> Cardinality,
) -> ultragraphdb_core_mapping::PropertyGraph {
    let mut d = CanonicalDataset::new();
    d.apply(
        &changes.into_iter().fold(Changeset::new(0), Changeset::with),
        cardinality,
    )
    .unwrap();
    let statements: Vec<_> = d.statements().collect();
    property_graph_view(
        &statements,
        graph,
        &Names::new("http://ex.org/"),
        cardinality,
    )
}

fn insert(
    g: GraphName,
    s: impl Into<NamedOrBlankNode>,
    p: NamedNode,
    o: impl Into<Term>,
) -> Change {
    Change::Insert {
        graph: g,
        subject: s.into(),
        predicate: p,
        object: o.into(),
    }
}

#[test]
fn test_a_single_valued_property_is_a_scalar_and_a_literal_type_a_property() {
    let g = GraphName::DefaultGraph;
    let pg = view(
        vec![
            insert(g.clone(), iri("a"), iri("age"), Literal::from(30)),
            insert(
                g.clone(),
                iri("a"),
                rdf::TYPE.into_owned(),
                Literal::new_simple_literal("odd"),
            ),
        ],
        &g,
        |p| {
            if *p == iri("age") {
                Cardinality::Single
            } else {
                Cardinality::Multi
            }
        },
    );
    let a = pg.node("<http://ex.org/a>").unwrap();
    assert_eq!(
        a.properties().get("age"),
        Some(&PgValue::Scalar(Literal::from(30)))
    );
    assert_eq!(
        a.properties().get(rdf::TYPE.as_str()),
        Some(&PgValue::List(vec![Literal::new_simple_literal("odd")]))
    );
    assert!(a.labels().is_empty());
}

#[test]
fn test_an_edges_target_is_a_node_and_another_graph_is_left_out() {
    let g = GraphName::DefaultGraph;
    let other = GraphName::NamedNode(iri("other"));
    let b = BlankNode::new_unchecked("b");
    let pg = view(
        vec![
            insert(g.clone(), iri("a"), iri("knows"), b.clone()),
            insert(other.clone(), iri("x"), iri("knows"), iri("y")),
        ],
        &g,
        |_| Cardinality::Multi,
    );
    assert_eq!(pg.nodes().count(), 2);
    assert!(pg.node(&b.to_string()).is_some());
    assert_eq!(pg.edges().len(), 1);
    assert_eq!(pg.edges()[0].edge_type(), "knows");
    assert!(pg.node("<http://ex.org/x>").is_none());
}
