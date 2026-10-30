/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Declared, DeclaredEntityType, DeclaredKind, DeclaredPredicate,
    DeclaredRelation, Direction, RequestTerm, StatementTarget, TermTable, Write, WriteChange,
    WriteView,
};

use crate::utils::golden_bodies::{frame_of, golden};

const EX: &str = "http://ex.org/";

fn ex(name: &str) -> String {
    format!("{EX}{name}")
}

const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

fn literal(lexical: &str, datatype: Option<&str>) -> RequestTerm {
    RequestTerm::Literal {
        lexical: lexical.to_string(),
        datatype: datatype.map(str::to_string),
    }
}

fn tagged(lexical: &str, language: &str, direction: Option<Direction>) -> RequestTerm {
    RequestTerm::LanguageLiteral {
        lexical: lexical.to_string(),
        language: language.to_string(),
        direction,
    }
}

/// The golden Write, its terms interned in generate_bodies.py's order of first use.
pub fn golden_write() -> Write {
    let mut t = TermTable::new();
    let mut add = |term: RequestTerm| t.intern(term).unwrap();
    let a = add(RequestTerm::Iri(ex("a")));
    let knows = add(RequestTerm::Iri(ex("knows")));
    let b = add(RequestTerm::Iri(ex("b")));
    let g = add(RequestTerm::Iri(ex("g")));
    let b1 = add(RequestTerm::BlankNode("b1".to_string()));
    let nick = add(RequestTerm::Iri(ex("nick")));
    let x_en = add(tagged("x", "en", None));
    let c = add(RequestTerm::Iri(ex("c")));
    let since = add(RequestTerm::Iri(ex("since")));
    let year = add(literal("2020", Some(&format!("{XSD}gYear"))));
    let weight = add(RequestTerm::Iri(ex("weight")));
    let half = add(literal("0.5", Some(&format!("{XSD}decimal"))));
    let says = add(RequestTerm::Iri(ex("says")));
    let triple = add(RequestTerm::Triple {
        subject: a,
        predicate: knows,
        object: b,
    });
    let label = add(RequestTerm::Iri(ex("label")));
    let plain = add(literal("plain", None));
    let rtl = add(tagged("مرحبا", "ar", Some(Direction::Rtl)));
    let changes = vec![
        WriteChange::Insert {
            graph: None,
            subject: a,
            predicate: knows,
            object: b,
        },
        WriteChange::Delete {
            graph: Some(g),
            subject: b1,
            predicate: nick,
            object: x_en,
        },
        WriteChange::DeletePattern {
            graph: None,
            subject: a,
            predicate: nick,
        },
        WriteChange::DeleteStatement {
            graph: None,
            id: 0x0000_0005_0000_0007,
        },
        WriteChange::InsertEdge {
            graph: None,
            subject: a,
            predicate: knows,
            object: c,
            properties: vec![(since, year)],
        },
        WriteChange::SetProperty {
            statement: StatementTarget::Id { graph: None, id: 9 },
            predicate: weight,
            value: half,
        },
        WriteChange::DeleteProperty {
            statement: StatementTarget::Triple {
                graph: None,
                subject: a,
                predicate: knows,
                object: b,
            },
            predicate: since,
            value: None,
        },
        WriteChange::Insert {
            graph: None,
            subject: a,
            predicate: says,
            object: triple,
        },
        WriteChange::Insert {
            graph: None,
            subject: a,
            predicate: label,
            object: plain,
        },
        WriteChange::Insert {
            graph: None,
            subject: a,
            predicate: label,
            object: rtl,
        },
        WriteChange::Declare(Declared::Predicate(
            DeclaredPredicate::new(ex("name"))
                .with_kind(DeclaredKind::Property)
                .with_single(true)
                .with_datatype(format!("{XSD}string"))
                .with_unique(true)
                .with_tokenizers(vec!["term".to_string(), "exact".to_string()])
                .with_count_index(true)
                .with_domains(vec![ex("Person")]),
        )),
        WriteChange::Declare(Declared::Predicate(
            DeclaredPredicate::new(ex("embedding"))
                .with_kind(DeclaredKind::Either)
                .with_vector_index(
                    "hnsw",
                    vec![
                        ("metric".to_string(), "euclidean".to_string()),
                        ("maxLevels".to_string(), "3".to_string()),
                    ],
                ),
        )),
        WriteChange::Declare(Declared::EntityType(DeclaredEntityType::new(
            ex("Person"),
            Some(ex("Agent")),
            false,
        ))),
        WriteChange::Declare(Declared::Relation(DeclaredRelation::new(
            ex("Employment"),
            vec![ex("employee"), ex("employer")],
        ))),
    ];
    Write::new(t, changes)
}

#[test]
fn test_write_builds_the_golden_frame() {
    let write = golden_write();
    assert!(
        write.command_header().iter().all(|b| *b == 0),
        "a Write's command header is all zero"
    );
    assert_eq!(write.terms().len(), 17);
    assert_eq!(write.changes().len(), 14);
    assert_eq!(
        write.frame(golden("write").header()).unwrap(),
        golden("write").bytes()
    );
}

#[test]
fn test_an_empty_write_is_valid() {
    let bytes = Write::new(TermTable::new(), Vec::new())
        .frame(golden("write").header())
        .unwrap();
    let view = WriteView::from_frame(&frame_of(&bytes)).unwrap();
    assert!(view.is_empty());
    assert_eq!(
        view.to_body().unwrap(),
        Write::new(TermTable::new(), Vec::new())
    );
}
