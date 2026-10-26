/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The mapping-contract scenarios, over the canonical statements in memory.

use cucumber::{World, then, when};
use oxrdf::vocab::{rdf, xsd};
use oxrdf::{BlankNode, GraphName, Literal, NamedNode, NamedOrBlankNode, Quad, Term};
use ultragraphdb_core_mapping::{
    CanonicalDataset, Cardinality, Change, Changeset, Names, PgEdge, PgNode, PgValue,
    PropertyGraph, Statement, StatementRef, property_graph_to_changeset, property_graph_view,
    quads_to_changeset, rdf_view,
};

const BASE: &str = "http://ex.org/";

#[derive(Debug, Default, World)]
struct Steps {
    dataset: CanonicalDataset,
    written: Vec<Quad>,
    pg_written: PropertyGraph,
}

fn iri(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("{BASE}{name}"))
}

fn node(name: &str) -> NamedOrBlankNode {
    iri(name).into()
}

fn multi(_: &NamedNode) -> Cardinality {
    Cardinality::Multi
}

impl Steps {
    fn apply(&mut self, changes: Vec<Change>) {
        let mut changeset = Changeset::new(0);
        changes.into_iter().for_each(|c| changeset.push(c));
        self.dataset.apply(&changeset, multi).expect("applies");
    }

    fn statements(&self) -> Vec<Statement> {
        self.dataset.statements().collect()
    }

    fn quads(&self) -> Vec<Quad> {
        rdf_view(&self.statements())
    }

    fn pg(&self) -> PropertyGraph {
        property_graph_view(
            &self.statements(),
            &GraphName::DefaultGraph,
            &Names::new(BASE),
            multi,
        )
    }
}

fn insert(s: &str, p: &str, o: Term) -> Change {
    Change::Insert {
        graph: GraphName::DefaultGraph,
        subject: node(s),
        predicate: iri(p),
        object: o,
    }
}

fn edge(s: &str, p: &str, o: &str) -> Change {
    Change::InsertEdge {
        graph: GraphName::DefaultGraph,
        subject: node(s),
        predicate: iri(p),
        object: iri(o).into(),
        properties: Vec::new(),
    }
}

#[when("the statement <a> <knows> <b> is written to the default graph")]
fn write_plain(w: &mut Steps) {
    w.apply(vec![
        insert("a", "knows", iri("b").into()),
        insert("a", "knows", iri("b").into()),
    ]);
}

#[then("it is held once, with no statement ID")]
fn held_once(w: &mut Steps) {
    let statements = w.statements();
    assert_eq!(statements.len(), 1);
    assert_eq!(statements[0].id(), None);
}

#[when("<a> <knows> <b> is written twice to the default graph as two distinct edges")]
#[when("<a> <knows> <b> is held twice with two statement IDs")]
fn write_parallel(w: &mut Steps) {
    w.apply(vec![edge("a", "knows", "b"), edge("a", "knows", "b")]);
}

#[then("both statements are held, each with its own statement ID")]
fn both_ids(w: &mut Steps) {
    let ids: Vec<_> = w.statements().iter().map(Statement::id).collect();
    assert_eq!(ids.len(), 2);
    assert!(
        ids.iter().all(Option::is_some) && ids[0] != ids[1],
        "{ids:?}"
    );
}

#[then("the RDF view holds the quad <a> <knows> <b> once, with two reifiers")]
fn one_quad_two_reifiers(w: &mut Steps) {
    let quads = w.quads();
    let plain = Quad::new(node("a"), iri("knows"), iri("b"), GraphName::DefaultGraph);
    assert_eq!(quads.iter().filter(|q| **q == plain).count(), 1);
    assert_eq!(
        quads.iter().filter(|q| q.predicate == rdf::REIFIES).count(),
        2
    );
}

#[when("the statement <a> <knows> <b> carries the property <since> \"2020\"^^xsd:gYear")]
fn with_property(w: &mut Steps) {
    w.apply(vec![
        insert("a", "knows", iri("b").into()),
        Change::SetProperty {
            statement: StatementRef::Triple {
                graph: GraphName::DefaultGraph,
                subject: node("a"),
                predicate: iri("knows"),
                object: iri("b").into(),
            },
            predicate: iri("since"),
            value: Literal::new_typed_literal("2020", xsd::G_YEAR).into(),
        },
    ]);
}

#[then(
    "the RDF view holds the quad and a reifier of it annotated with <since> \"2020\"^^xsd:gYear"
)]
fn annotated(w: &mut Steps) {
    let quads = w.quads();
    assert!(quads.contains(&Quad::new(
        node("a"),
        iri("knows"),
        iri("b"),
        GraphName::DefaultGraph
    )));
    let reifier = quads
        .iter()
        .find(|q| q.predicate == rdf::REIFIES)
        .map(|q| q.subject.clone())
        .expect("a reifier");
    let annotation = Quad::new(
        reifier,
        iri("since"),
        Literal::new_typed_literal("2020", xsd::G_YEAR),
        GraphName::DefaultGraph,
    );
    assert!(quads.contains(&annotation), "{quads:?}");
}

#[when("<a> rdf:type <Person> is held")]
fn typed(w: &mut Steps) {
    w.apply(vec![Change::Insert {
        graph: GraphName::DefaultGraph,
        subject: node("a"),
        predicate: rdf::TYPE.into_owned(),
        object: iri("Person").into(),
    }]);
}

#[then("the property-graph view shows node a with the label Person")]
fn labelled(w: &mut Steps) {
    let pg = w.pg();
    let a = pg.node(&node("a").to_string()).expect("node a");
    assert!(a.labels().contains("Person"));
    assert!(pg.edges().is_empty(), "a label is no edge");
}

#[then("the property-graph view shows two knows edges from a to b")]
fn two_edges(w: &mut Steps) {
    let pg = w.pg();
    let edges: Vec<&PgEdge> = pg
        .edges()
        .iter()
        .filter(|e| e.edge_type() == "knows")
        .collect();
    assert_eq!(edges.len(), 2);
    assert!(
        edges
            .iter()
            .all(|e| *e.source() == node("a") && *e.target() == node("b"))
    );
    assert_ne!(edges[0].id(), edges[1].id());
}

#[when("<a> <nick> \"Al\" and <a> <nick> \"Ally\" are held and nick is not declared single-valued")]
fn nicks(w: &mut Steps) {
    w.apply(vec![
        insert("a", "nick", Literal::new_simple_literal("Ally").into()),
        insert("a", "nick", Literal::new_simple_literal("Al").into()),
    ]);
}

#[then("the property-graph view shows nick on a as a list of both values")]
fn nick_list(w: &mut Steps) {
    let pg = w.pg();
    let a = pg.node(&node("a").to_string()).expect("node a");
    assert_eq!(
        a.properties().get("nick"),
        Some(&PgValue::List(vec![
            Literal::new_simple_literal("Al"),
            Literal::new_simple_literal("Ally")
        ]))
    );
}

#[when(
    "the base IRI is http://ex.org/ and a property age is written through the property-graph view"
)]
fn pg_age(w: &mut Steps) {
    let pg = PropertyGraph::new()
        .with_node(PgNode::new(node("a")).with_property("age", PgValue::Scalar(Literal::from(30))));
    let changeset = property_graph_to_changeset(
        &pg,
        0,
        &GraphName::DefaultGraph,
        &Names::new("http://ex.org/"),
    );
    w.dataset.apply(&changeset, multi).expect("applies");
}

#[then("the RDF view holds it under the predicate <http://ex.org/age>")]
fn age_iri(w: &mut Steps) {
    let quads = w.quads();
    assert_eq!(quads.len(), 1);
    assert_eq!(
        quads[0].predicate,
        NamedNode::new_unchecked("http://ex.org/age")
    );
}

#[when("_:x <knows> <b> is written through the RDF view and read back through it")]
fn blank_written(w: &mut Steps) {
    let x = BlankNode::new_unchecked("x");
    w.written = vec![
        Quad::new(x.clone(), iri("knows"), iri("b"), GraphName::DefaultGraph),
        Quad::new(
            x,
            iri("name"),
            Literal::new_simple_literal("X"),
            GraphName::DefaultGraph,
        ),
    ];
    let changeset = quads_to_changeset(0, &w.written).unwrap();
    w.dataset.apply(&changeset, multi).unwrap();
}

#[then(
    "the subject is presented as a blank node, the same blank node for every statement that named _:x"
)]
fn same_blank(w: &mut Steps) {
    let subjects: Vec<NamedOrBlankNode> = w.quads().into_iter().map(|q| q.subject).collect();
    assert_eq!(subjects.len(), 2);
    assert!(matches!(subjects[0], NamedOrBlankNode::BlankNode(_)));
    assert_eq!(subjects[0], subjects[1]);
}

#[when(expr = "<a> <n> {string}^^xsd:integer is written")]
fn integer_written(w: &mut Steps, lexical: String) {
    w.apply(vec![insert(
        "a",
        "n",
        Literal::new_typed_literal(lexical, xsd::INTEGER).into(),
    )]);
}

#[then(
    expr = "the RDF view returns {string}^^xsd:integer, a term distinct from {string}^^xsd:integer"
)]
fn integer_kept_distinct(w: &mut Steps, lexical: String, other: String) {
    let object = w.quads()[0].object.clone();
    assert_eq!(
        object,
        Literal::new_typed_literal(lexical, xsd::INTEGER).into()
    );
    assert_ne!(
        object,
        Literal::new_typed_literal(other, xsd::INTEGER).into()
    );
}

#[then(expr = "the RDF view returns {string}^^xsd:integer unchanged")]
fn integer_unchanged(w: &mut Steps, lexical: String) {
    assert_eq!(
        w.quads()[0].object,
        Literal::new_typed_literal(lexical, xsd::INTEGER).into()
    );
}

#[when(
    "a set of quads over two named graphs, with blank nodes, language-tagged and typed literals, is written and read back through the RDF view"
)]
fn quads_written(w: &mut Steps) {
    let (g1, g2) = (
        GraphName::NamedNode(iri("g1")),
        GraphName::NamedNode(iri("g2")),
    );
    let b = BlankNode::new_unchecked("b0");
    w.written = vec![
        Quad::new(node("a"), iri("knows"), b.clone(), g1.clone()),
        Quad::new(
            b.clone(),
            iri("name"),
            Literal::new_language_tagged_literal("Anne", "fr").unwrap(),
            g1.clone(),
        ),
        Quad::new(
            node("a"),
            iri("age"),
            Literal::new_typed_literal("042", xsd::INTEGER),
            g2.clone(),
        ),
        Quad::new(
            node("a"),
            iri("age"),
            Literal::new_typed_literal("42", xsd::INTEGER),
            g2.clone(),
        ),
        Quad::new(node("a"), iri("knows"), b, g2),
        Quad::new(
            node("a"),
            iri("seen"),
            Literal::new_typed_literal("2026-10-26", xsd::DATE),
            GraphName::DefaultGraph,
        ),
    ];
    let changeset = quads_to_changeset(0, &w.written).unwrap();
    w.dataset.apply(&changeset, multi).unwrap();
}

#[then("the quads read back equal the quads written")]
fn quads_equal(w: &mut Steps) {
    let mut written = w.written.clone();
    written.sort_by_key(Quad::to_string);
    assert_eq!(w.quads(), written);
}

#[when(
    "nodes with labels and properties, and parallel edges with properties, are written and read back through the property-graph view"
)]
fn pg_written(w: &mut Steps) {
    let since = |y: &str| vec![Term::from(Literal::new_typed_literal(y, xsd::G_YEAR))];
    w.pg_written = PropertyGraph::new()
        .with_node(
            PgNode::new(node("a"))
                .with_label("Person")
                .with_label("Author")
                .with_property("name", PgValue::Scalar(Literal::new_simple_literal("Ann"))),
        )
        .with_node(PgNode::new(node("b")).with_label("Person").with_property(
            "nick",
            PgValue::List(vec![
                Literal::new_simple_literal("B"),
                Literal::new_simple_literal("Bee"),
            ]),
        ))
        .with_edge(PgEdge::new(node("a"), "knows", node("b")).with_property("since", since("2020")))
        .with_edge(PgEdge::new(node("a"), "knows", node("b")).with_property("since", since("2024")))
        .with_edge(PgEdge::new(node("b"), "likes", node("a")));
    let changeset = property_graph_to_changeset(
        &w.pg_written,
        0,
        &GraphName::DefaultGraph,
        &Names::new(BASE),
    );
    w.dataset
        .apply(&changeset, |p: &NamedNode| {
            if *p == iri("name") {
                Cardinality::Single
            } else {
                Cardinality::Multi
            }
        })
        .unwrap();
}

#[then("the nodes, labels, properties, edges and edge properties read back equal those written")]
fn pg_equal(w: &mut Steps) {
    let read = property_graph_view(
        &w.statements(),
        &GraphName::DefaultGraph,
        &Names::new(BASE),
        |p: &NamedNode| {
            if *p == iri("name") {
                Cardinality::Single
            } else {
                Cardinality::Multi
            }
        },
    );
    assert!(
        read.edges()
            .iter()
            .filter(|e| e.edge_type() == "knows")
            .all(|e| e.id().is_some())
    );
    assert_eq!(
        read.without_edge_ids(),
        w.pg_written.clone().without_edge_ids()
    );
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_test_bdd::features_dir()),
    );
}
