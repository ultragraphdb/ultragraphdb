/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::vocab::{rdf, xsd};
use oxrdf::{BaseDirection, BlankNode, GraphName, Literal, NamedNode, Quad, Term, Triple};
use ultragraphdb_core_mapping::{
    CanonicalDataset, Cardinality, Change, Changeset, MappingError, StatementId,
    quads_to_changeset, rdf_view, statement_iri,
};

fn iri(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

#[test]
fn test_every_kind_of_literal_and_graph_round_trips() {
    let g = GraphName::NamedNode(iri("g"));
    let b = BlankNode::new_unchecked("n");
    let literals = vec![
        Literal::new_simple_literal("plain"),
        Literal::new_language_tagged_literal("hello", "en-GB").unwrap(),
        Literal::new_directional_language_tagged_literal("שלום", "he", BaseDirection::Rtl).unwrap(),
        Literal::new_typed_literal("01", xsd::INTEGER),
        Literal::new_typed_literal("twelve", xsd::INTEGER),
        Literal::new_typed_literal("x", iri("custom")),
    ];
    let mut quads: Vec<Quad> = literals
        .into_iter()
        .map(|l| Quad::new(b.clone(), iri("v"), l, g.clone()))
        .collect();
    quads.push(Quad::new(
        iri("a"),
        iri("knows"),
        b.clone(),
        GraphName::DefaultGraph,
    ));
    quads.push(Quad::new(
        iri("a"),
        iri("knows"),
        b,
        GraphName::DefaultGraph,
    ));
    let mut d = CanonicalDataset::new();
    d.apply(&quads_to_changeset(0, &quads).unwrap(), |_| {
        Cardinality::Multi
    })
    .unwrap();
    let statements: Vec<_> = d.statements().collect();
    let read = rdf_view(&statements);
    quads.dedup();
    quads.sort_by_key(Quad::to_string);
    assert_eq!(read, quads);
}

#[test]
fn test_a_statement_id_presents_as_a_reifier_with_its_annotations() {
    let mut d = CanonicalDataset::new();
    let changeset = Changeset::new(0).with(Change::InsertEdge {
        graph: GraphName::DefaultGraph,
        subject: iri("a").into(),
        predicate: iri("knows"),
        object: iri("b").into(),
        properties: vec![(iri("w"), Literal::from(3).into())],
    });
    d.apply(&changeset, |_| Cardinality::Multi).unwrap();
    let statements: Vec<_> = d.statements().collect();
    let reifier = statement_iri(StatementId::new(1));
    // `ugdb:statement/1`, `ugdb:` being PROTOCOL.md section 10.4's https://w3id.org/ultragraphdb/ns/.
    assert_eq!(
        reifier.as_str(),
        "https://w3id.org/ultragraphdb/ns/statement/1"
    );
    let triple = Term::Triple(Box::new(Triple::new(iri("a"), iri("knows"), iri("b"))));
    let read = rdf_view(&statements);
    assert_eq!(read.len(), 3);
    assert!(read.contains(&Quad::new(
        reifier.clone(),
        rdf::REIFIES,
        triple.clone(),
        GraphName::DefaultGraph
    )));
    assert!(read.contains(&Quad::new(
        reifier,
        iri("w"),
        Literal::from(3),
        GraphName::DefaultGraph
    )));
    let inner = Triple::new(iri("a"), iri("knows"), iri("b"));
    let refused = quads_to_changeset(
        0,
        &[
            Quad::new(iri("a"), iri("knows"), iri("b"), GraphName::DefaultGraph),
            Quad::new(iri("r"), rdf::REIFIES, triple, GraphName::DefaultGraph),
        ],
    );
    assert_eq!(
        refused,
        Err(MappingError::UnsupportedObject(inner.to_string())),
        "a triple term is presented, never written"
    );
}

#[test]
fn test_a_reifier_and_its_annotations_are_in_their_statements_graph() {
    let g = GraphName::NamedNode(iri("g"));
    let mut d = CanonicalDataset::new();
    let changeset = Changeset::new(0).with(Change::InsertEdge {
        graph: g.clone(),
        subject: iri("a").into(),
        predicate: iri("knows"),
        object: iri("b").into(),
        properties: vec![(iri("w"), Literal::from(3).into())],
    });
    d.apply(&changeset, |_| Cardinality::Multi).unwrap();
    let statements: Vec<_> = d.statements().collect();
    let reifier = statement_iri(StatementId::new(1));
    let triple = Term::Triple(Box::new(Triple::new(iri("a"), iri("knows"), iri("b"))));
    let mut expected = vec![
        Quad::new(iri("a"), iri("knows"), iri("b"), g.clone()),
        Quad::new(reifier.clone(), rdf::REIFIES, triple, g.clone()),
        Quad::new(reifier, iri("w"), Literal::from(3), g),
    ];
    expected.sort_by_key(Quad::to_string);
    assert_eq!(rdf_view(&statements), expected);
}

#[test]
fn test_each_quad_writes_as_one_insert_in_its_graph() {
    let g = GraphName::NamedNode(iri("g"));
    let quads = [
        Quad::new(iri("a"), iri("knows"), iri("b"), g.clone()),
        Quad::new(
            iri("a"),
            iri("name"),
            Literal::new_simple_literal("A"),
            GraphName::DefaultGraph,
        ),
    ];
    let changeset = quads_to_changeset(5, &quads).unwrap();
    assert_eq!(changeset.namespace(), 5);
    assert_eq!(
        changeset.changes(),
        &[
            Change::Insert {
                graph: g,
                subject: iri("a").into(),
                predicate: iri("knows"),
                object: iri("b").into(),
            },
            Change::Insert {
                graph: GraphName::DefaultGraph,
                subject: iri("a").into(),
                predicate: iri("name"),
                object: Literal::new_simple_literal("A").into(),
            },
        ]
    );
    assert!(quads_to_changeset(5, &[]).unwrap().is_empty());
}
