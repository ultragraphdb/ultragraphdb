/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::vocab::xsd;
use oxrdf::{BlankNode, GraphName, Literal, NamedNode};
use ultragraphdb_core_mapping::{
    Cardinality, Change, Changeset, Declaration, PredicateDeclaration, PredicateKind,
    RelationDeclaration, StatementId, StatementRef, TypeDeclaration, VectorIndexSpec,
};
use ultragraphdb_protocol_wire::{BodyBuilder, Write};

use crate::utils::golden_bodies::golden;

pub fn ex(s: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{s}"))
}

/// The golden Write's changeset, as generate_bodies.py lays it out.
pub fn golden_changeset() -> Changeset {
    let d = GraphName::DefaultGraph;
    Changeset::new(2)
        .with(Change::Insert {
            graph: d.clone(),
            subject: ex("a").into(),
            predicate: ex("knows"),
            object: ex("b").into(),
        })
        .with(Change::Delete {
            graph: ex("g").into(),
            subject: BlankNode::new_unchecked("b1").into(),
            predicate: ex("nick"),
            object: Literal::new_language_tagged_literal_unchecked("x", "en").into(),
        })
        .with(Change::DeletePattern {
            graph: d.clone(),
            subject: ex("a").into(),
            predicate: ex("nick"),
        })
        .with(Change::DeleteStatement {
            graph: d.clone(),
            id: StatementId::new(0x0000_0005_0000_0007),
        })
        .with(Change::InsertEdge {
            graph: d.clone(),
            subject: ex("a").into(),
            predicate: ex("knows"),
            object: ex("c").into(),
            properties: vec![(
                ex("since"),
                Literal::new_typed_literal("2020", xsd::G_YEAR).into(),
            )],
        })
        .with(Change::SetProperty {
            statement: StatementRef::Id {
                graph: d.clone(),
                id: StatementId::new(9),
            },
            predicate: ex("weight"),
            value: Literal::new_typed_literal("0.5", xsd::DECIMAL).into(),
        })
        .with(Change::DeleteProperty {
            statement: StatementRef::Triple {
                graph: d.clone(),
                subject: ex("a").into(),
                predicate: ex("knows"),
                object: ex("b").into(),
            },
            predicate: ex("since"),
            value: None,
        })
        .with(Change::Declare(Declaration::Predicate(
            PredicateDeclaration::new(ex("name"))
                .with_kind(PredicateKind::Property)
                .with_cardinality(Cardinality::Single)
                .with_datatype(xsd::STRING.into_owned())
                .with_unique(true)
                .with_tokenizers(vec!["term".to_string(), "exact".to_string()])
                .with_count_index(true)
                .with_domains(vec![ex("Person")]),
        )))
        .with(Change::Declare(Declaration::Predicate(
            PredicateDeclaration::new(ex("embedding"))
                .with_kind(PredicateKind::Either)
                .with_vector_index(VectorIndexSpec::new(
                    "hnsw",
                    vec![
                        ("metric".to_string(), "euclidean".to_string()),
                        ("maxLevels".to_string(), "3".to_string()),
                    ],
                )),
        )))
        .with(Change::Declare(Declaration::EntityType(
            TypeDeclaration::new(ex("Person"), Some(ex("Agent")), false),
        )))
        .with(Change::Declare(Declaration::Relation(
            RelationDeclaration::new(ex("Employment"), vec![ex("employee"), ex("employer")]),
        )))
}

#[test]
fn test_write_builds_the_golden_frame() {
    let write = Write::new(golden_changeset());
    assert_eq!(write.changeset(), &golden_changeset());
    assert_eq!(
        write.frame(golden("write").header()).unwrap(),
        golden("write").bytes()
    );
}

#[test]
fn test_an_empty_changeset_is_a_valid_write() {
    use ultragraphdb_protocol_wire::{BodyView, WriteView};
    let bytes = Write::new(Changeset::new(9))
        .frame(golden("write").header())
        .unwrap();
    let view = WriteView::from_frame(&crate::utils::golden_bodies::frame_of(&bytes)).unwrap();
    assert!(view.is_empty());
    assert_eq!(view.namespace(), 9);
    assert_eq!(view.to_body().unwrap(), Write::new(Changeset::new(9)));
}
