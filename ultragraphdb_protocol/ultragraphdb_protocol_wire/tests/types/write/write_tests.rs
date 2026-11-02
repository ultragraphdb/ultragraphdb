/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Declared, DeclaredEntityType, DeclaredKind, DeclaredPredicate,
    DeclaredRelation, Direction, RequestTerm, StatementTarget, TermTable, Write, WriteChange,
    WriteView,
};
use ultragraphdb_tool_bdd::TestResult;

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

/// The golden Write's term indices, by the names generate_bodies.py gives them.
struct GoldenTerms {
    a: u32,
    knows: u32,
    b: u32,
    g: u32,
    b1: u32,
    nick: u32,
    x_en: u32,
    c: u32,
    since: u32,
    year: u32,
    weight: u32,
    half: u32,
    says: u32,
    triple: u32,
    label: u32,
    plain: u32,
    rtl: u32,
}

/// The golden Write, its terms interned in generate_bodies.py's order of first use.
pub fn golden_write() -> TestResult<Write> {
    let (table, terms) = golden_terms()?;
    let mut changes = golden_statements(&terms);
    changes.extend(golden_declarations());
    Ok(Write::new(table, changes))
}

/// The golden Write's term table, interned in generate_bodies.py's order of first use, and its indices.
fn golden_terms() -> TestResult<(TermTable, GoldenTerms)> {
    let mut t = TermTable::new();
    let mut add = |term: RequestTerm| t.intern(term);
    let a = add(RequestTerm::Iri(ex("a")))?;
    let knows = add(RequestTerm::Iri(ex("knows")))?;
    let b = add(RequestTerm::Iri(ex("b")))?;
    let g = add(RequestTerm::Iri(ex("g")))?;
    let b1 = add(RequestTerm::BlankNode("b1".to_string()))?;
    let nick = add(RequestTerm::Iri(ex("nick")))?;
    let x_en = add(tagged("x", "en", None))?;
    let c = add(RequestTerm::Iri(ex("c")))?;
    let since = add(RequestTerm::Iri(ex("since")))?;
    let year = add(literal("2020", Some(&format!("{XSD}gYear"))))?;
    let weight = add(RequestTerm::Iri(ex("weight")))?;
    let half = add(literal("0.5", Some(&format!("{XSD}decimal"))))?;
    let says = add(RequestTerm::Iri(ex("says")))?;
    let triple = add(RequestTerm::Triple {
        subject: a,
        predicate: knows,
        object: b,
    })?;
    let label = add(RequestTerm::Iri(ex("label")))?;
    let plain = add(literal("plain", None))?;
    let rtl = add(tagged("مرحبا", "ar", Some(Direction::Rtl)))?;
    Ok((
        t,
        GoldenTerms {
            a,
            knows,
            b,
            g,
            b1,
            nick,
            x_en,
            c,
            since,
            year,
            weight,
            half,
            says,
            triple,
            label,
            plain,
            rtl,
        },
    ))
}

/// The golden Write's statement changes, over its terms `t`.
fn golden_statements(t: &GoldenTerms) -> Vec<WriteChange> {
    vec![
        WriteChange::Insert {
            graph: None,
            subject: t.a,
            predicate: t.knows,
            object: t.b,
        },
        WriteChange::Delete {
            graph: Some(t.g),
            subject: t.b1,
            predicate: t.nick,
            object: t.x_en,
        },
        WriteChange::DeletePattern {
            graph: None,
            subject: t.a,
            predicate: t.nick,
        },
        WriteChange::DeleteStatement {
            graph: None,
            id: 0x0000_0005_0000_0007,
        },
        WriteChange::InsertEdge {
            graph: None,
            subject: t.a,
            predicate: t.knows,
            object: t.c,
            properties: vec![(t.since, t.year)],
        },
        WriteChange::SetProperty {
            statement: StatementTarget::Id { graph: None, id: 9 },
            predicate: t.weight,
            value: t.half,
        },
        WriteChange::DeleteProperty {
            statement: StatementTarget::Triple {
                graph: None,
                subject: t.a,
                predicate: t.knows,
                object: t.b,
            },
            predicate: t.since,
            value: None,
        },
        WriteChange::Insert {
            graph: None,
            subject: t.a,
            predicate: t.says,
            object: t.triple,
        },
        WriteChange::Insert {
            graph: None,
            subject: t.a,
            predicate: t.label,
            object: t.plain,
        },
        WriteChange::Insert {
            graph: None,
            subject: t.a,
            predicate: t.label,
            object: t.rtl,
        },
    ]
}

/// The golden Write's declarations.
fn golden_declarations() -> Vec<WriteChange> {
    vec![
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
    ]
}

#[test]
fn test_write_builds_the_golden_frame() -> TestResult {
    let write = golden_write()?;
    assert!(
        write.command_header().iter().all(|b| *b == 0),
        "a Write's command header is all zero"
    );
    assert_eq!(write.terms().len(), 17);
    assert_eq!(write.changes().len(), 14);
    assert_eq!(
        write.frame(golden("write")?.header())?,
        golden("write")?.bytes()?
    );
    Ok(())
}

#[test]
fn test_an_empty_write_is_valid() -> TestResult {
    let bytes = Write::new(TermTable::new(), Vec::new()).frame(golden("write")?.header())?;
    let view = WriteView::from_frame(&frame_of(&bytes)?)?;
    assert!(view.is_empty());
    assert_eq!(view.to_body()?, Write::new(TermTable::new(), Vec::new()));
    Ok(())
}
