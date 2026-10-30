/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::vocab::rdf;
use oxrdf::{BlankNode, GraphName, Literal, NamedNode, NamedOrBlankNode, Term};
use ultragraphdb_core_mapping::{
    CanonicalDataset, Cardinality, Change, Changeset, Names, PgEdge, PgNode, PgValue,
    PropertyGraph, StatementId, property_graph_to_changeset, property_graph_view,
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
        &Names::new("http://ex.org/").unwrap(),
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
    assert_eq!(pg.edges()[0].source(), &NamedOrBlankNode::from(iri("a")));
    assert_eq!(pg.edges()[0].target(), &NamedOrBlankNode::from(b));
    assert!(pg.node("<http://ex.org/x>").is_none());
}

#[test]
fn test_an_edge_carries_its_id_and_every_value_of_each_property() {
    let g = GraphName::DefaultGraph;
    let pg = view(
        vec![Change::InsertEdge {
            graph: g.clone(),
            subject: iri("a").into(),
            predicate: iri("knows"),
            object: iri("b").into(),
            properties: vec![
                (iri("w"), Literal::from(2).into()),
                (iri("src"), iri("census").into()),
                (iri("w"), Literal::from(1).into()),
            ],
        }],
        &g,
        |_| Cardinality::Multi,
    );
    let expected = PgEdge::new(iri("a").into(), "knows", iri("b").into())
        .with_id(StatementId::new(1))
        .with_property("src", vec![iri("census").into()])
        .with_property("w", vec![Literal::from(1).into(), Literal::from(2).into()]);
    assert_eq!(pg.edges(), &[expected]);
}

#[test]
fn test_a_property_graph_writes_as_labels_values_and_edges_of_their_own() {
    let names = Names::new("http://ex.org/").unwrap();
    let g = GraphName::NamedNode(iri("g"));
    let pg = PropertyGraph::new()
        .with_node(
            PgNode::new(iri("a").into())
                .with_label("Person")
                .with_label("http://schema.org/Person")
                .with_property("age", PgValue::Scalar(Literal::from(30)))
                .with_property(
                    "nick",
                    PgValue::List(vec![
                        Literal::new_simple_literal("B"),
                        Literal::new_simple_literal("Bee"),
                    ]),
                ),
        )
        .with_edge(
            PgEdge::new(iri("a").into(), "knows", iri("b").into())
                .with_property("since", vec![Literal::from(2020).into()]),
        );
    let changeset = property_graph_to_changeset(&pg, 7, &g, &names);
    assert_eq!(changeset.namespace(), 7);
    let insert = |p: NamedNode, o: Term| Change::Insert {
        graph: g.clone(),
        subject: iri("a").into(),
        predicate: p,
        object: o,
    };
    // Labels, then properties by name and each list in order, then the edges; a name with a colon is its own IRI.
    assert_eq!(
        changeset.changes(),
        &[
            insert(rdf::TYPE.into_owned(), iri("Person").into()),
            insert(
                rdf::TYPE.into_owned(),
                NamedNode::new_unchecked("http://schema.org/Person").into()
            ),
            insert(iri("age"), Literal::from(30).into()),
            insert(iri("nick"), Literal::new_simple_literal("B").into()),
            insert(iri("nick"), Literal::new_simple_literal("Bee").into()),
            Change::InsertEdge {
                graph: g.clone(),
                subject: iri("a").into(),
                predicate: iri("knows"),
                object: iri("b").into(),
                properties: vec![(iri("since"), Literal::from(2020).into())],
            },
        ]
    );
}
