/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_frontend::{check_change, check_operation, check_table};
use ultragraphdb_core_terms::{FLOAT32_VECTOR, SYSTEM_VOCABULARY, format_vector};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, CommitOperation, Direction, ErrorCode, ErrorReply, FrameView,
    Header, Link, MAX_FRAME_SIZE, RequestTerm, StatementTarget, TermTable, Write, WriteChange,
    WriteView,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

const CLUSTER: u64 = 1;

/// What `check_table` says of a literal of `datatype`: its error code, `None` when it holds.
fn checked(lexical: &str, datatype: &str) -> TestResult<Option<ErrorCode>> {
    let mut table = TermTable::new();
    let change = WriteChange::Insert {
        graph: None,
        subject: table.iri("http://ex.org/a"),
        predicate: table.iri("http://ex.org/p"),
        object: table.intern(RequestTerm::Literal {
            lexical: lexical.to_string(),
            datatype: Some(format!("{XSD}{datatype}")),
        })?,
    };
    let frame = Write::new(table, vec![change]).frame(Header::new(CLUSTER, Command::Write))?;
    let view = FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)?;
    let write = WriteView::from_frame(&view)?;
    Ok(check_table(write.terms())
        .err()
        .map(|refused| refused.code()))
}

/// A decimal or an integer past what a by-value payload holds is in its lexical space, and kept by its lexical form.
#[test]
fn test_a_number_past_every_payload_is_still_a_lexical_form() -> TestResult {
    for (lexical, datatype) in [
        ("0.1234567890123456789", "decimal"),
        ("123456789012345678901234567890.5", "decimal"),
        ("-.5", "decimal"),
        ("+5.", "decimal"),
        ("99999999999999999999999", "integer"),
        ("-99999999999999999999999", "long"),
    ] {
        assert_eq!(checked(lexical, datatype)?, None, "{lexical}");
    }
    Ok(())
}

/// What lies outside a number's lexical space is refused.
#[test]
fn test_what_is_no_number_is_refused() -> TestResult {
    for (lexical, datatype) in [
        ("", "decimal"),
        (".", "decimal"),
        ("+", "decimal"),
        ("1e5", "decimal"),
        ("1.2.3", "decimal"),
        ("", "integer"),
        ("-", "integer"),
        ("1.5", "integer"),
        ("twelve", "integer"),
    ] {
        assert_eq!(
            checked(lexical, datatype)?,
            Some(ErrorCode::Refused),
            "{lexical}"
        );
    }
    Ok(())
}

/// Every datatype the XSD parser knows takes its lexical forms and refuses what is none, a vector its own.
#[test]
fn test_each_known_datatype_checks_its_lexical_form() -> TestResult {
    for (valid, datatype) in [
        ("true", "boolean"),
        ("1.5E3", "double"),
        ("1.5", "float"),
        ("2026-11-08T10:00:00Z", "dateTime"),
        ("10:00:00", "time"),
        ("2026-11-08", "date"),
        ("2026-11", "gYearMonth"),
        ("2026", "gYear"),
        ("--11-08", "gMonthDay"),
        ("---08", "gDay"),
        ("--11", "gMonth"),
        ("P1Y2M3DT4H", "duration"),
        ("P1Y2M", "yearMonthDuration"),
        ("PT4H", "dayTimeDuration"),
        ("2026-11-08T10:00:00Z", "dateTimeStamp"),
    ] {
        assert_eq!(checked(valid, datatype)?, None, "{valid} {datatype}");
        assert_eq!(
            checked("not one", datatype)?,
            Some(ErrorCode::Refused),
            "{datatype}"
        );
    }
    Ok(())
}

/// A datatype the XSD parser has no type for takes any form.
#[test]
fn test_a_datatype_without_a_parser_takes_any_form() -> TestResult {
    assert_eq!(checked("not one", "anyURI")?, None);
    Ok(())
}

/// The vector datatype takes a vector's text and refuses anything else.
#[test]
fn test_a_vector_checks_its_lexical_form() -> TestResult {
    let full = |lexical: &str| -> TestResult<Option<ErrorCode>> {
        checked_term(RequestTerm::Literal {
            lexical: lexical.to_string(),
            datatype: Some(FLOAT32_VECTOR.to_string()),
        })
    };
    assert_eq!(full(&format_vector([0.5, -1.0].into_iter()))?, None);
    assert_eq!(full("not one")?, Some(ErrorCode::Refused));
    Ok(())
}

/// What `check_table` says of a table holding `term` as an insert's object: its error code, `None` when it holds.
fn checked_term(term: RequestTerm) -> TestResult<Option<ErrorCode>> {
    let mut table = TermTable::new();
    let change = WriteChange::Insert {
        graph: None,
        subject: table.iri("http://ex.org/a"),
        predicate: table.iri("http://ex.org/p"),
        object: table.intern(term)?,
    };
    let frame = Write::new(table, vec![change]).frame(Header::new(CLUSTER, Command::Write))?;
    let view = FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)?;
    let write = WriteView::from_frame(&view)?;
    Ok(check_table(write.terms())
        .err()
        .map(|refused| refused.code()))
}

/// A language tag that is not well formed, an IRI that is not absolute, and a datatype that is no IRI are refused.
#[test]
fn test_a_malformed_tag_or_iri_is_refused() -> TestResult {
    for term in [
        RequestTerm::LanguageLiteral {
            lexical: "x".to_string(),
            language: "!!".to_string(),
            direction: Some(Direction::Ltr),
        },
        RequestTerm::Iri("not absolute".to_string()),
        RequestTerm::Literal {
            lexical: "x".to_string(),
            datatype: Some("not absolute".to_string()),
        },
    ] {
        assert_eq!(checked_term(term)?, Some(ErrorCode::Refused));
    }
    let tagged = RequestTerm::LanguageLiteral {
        lexical: "x".to_string(),
        language: "en".to_string(),
        direction: None,
    };
    assert_eq!(checked_term(tagged)?, None);
    Ok(())
}

/// The terms of the change and operation tests: an IRI, a blank node, a literal and a triple term, by index.
struct Terms {
    iri: u32,
    blank: u32,
    literal: u32,
    triple: u32,
}

/// What `check` says of the terms of a table holding [`Terms`], read from a Write's frame.
fn with_terms<T>(check: impl FnOnce(&WriteView<'_>, &Terms) -> T) -> TestResult<T> {
    let mut table = TermTable::new();
    let iri = table.iri("http://ex.org/a");
    let blank = table.intern(RequestTerm::BlankNode("b".to_string()))?;
    let literal = table.intern(RequestTerm::Literal {
        lexical: "x".to_string(),
        datatype: None,
    })?;
    let triple = table.intern(RequestTerm::Triple {
        subject: iri,
        predicate: iri,
        object: iri,
    })?;
    let terms = Terms {
        iri,
        blank,
        literal,
        triple,
    };
    let frame = Write::new(table, Vec::new()).frame(Header::new(CLUSTER, Command::Write))?;
    let view = FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)?;
    let write = WriteView::from_frame(&view)?;
    Ok(check(&write, &terms))
}

/// The index a refusal names, or `None` for a change that passes.
fn refused_at(checked: Result<(), ErrorReply>) -> Option<u64> {
    checked.err().map(|refused| refused.detail())
}

/// Each kind of change passes with every slot holding what it takes, and is refused naming the first term in a slot that
/// does not take it.
#[test]
fn test_each_change_checks_its_slots() -> TestResult {
    let outcomes = with_terms(|write, t| {
        let table = write.terms();
        change_cases(t)
            .into_iter()
            .chain(property_cases(t))
            .map(|(change, expected)| {
                (
                    refused_at(check_change(table, 0, &change)),
                    expected.map(u64::from),
                )
            })
            .collect::<Vec<_>>()
    })?;
    for (index, (got, expected)) in outcomes.into_iter().enumerate() {
        assert_eq!(got, expected, "change {index}");
    }
    Ok(())
}

/// One change of each kind but a property's over the terms `t`, with the term its check is refused naming, or `None`
/// where it passes.
fn change_cases(t: &Terms) -> [(WriteChange, Option<u32>); 5] {
    [
        (
            WriteChange::Delete {
                graph: None,
                subject: t.iri,
                predicate: t.iri,
                object: t.literal,
            },
            None,
        ),
        (
            WriteChange::InsertEdge {
                graph: Some(t.iri),
                subject: t.blank,
                predicate: t.iri,
                object: t.iri,
                properties: vec![(t.iri, t.literal)],
            },
            None,
        ),
        (
            WriteChange::InsertEdge {
                graph: None,
                subject: t.iri,
                predicate: t.iri,
                object: t.iri,
                properties: vec![(t.literal, t.literal)],
            },
            Some(t.literal),
        ),
        (
            WriteChange::DeleteStatement {
                graph: None,
                reifier: t.literal,
            },
            Some(t.literal),
        ),
        (
            WriteChange::DeletePattern {
                graph: None,
                subject: t.iri,
                predicate: t.blank,
            },
            Some(t.blank),
        ),
    ]
}

/// The property changes over the terms `t`, by a statement's reifier and by its quad, with the term each check is
/// refused naming, or `None` where it passes.
fn property_cases(t: &Terms) -> [(WriteChange, Option<u32>); 4] {
    let by_reifier = StatementTarget::Reifier {
        graph: Some(t.blank),
        reifier: t.iri,
    };
    let by_quad = |object| StatementTarget::Triple {
        graph: None,
        subject: t.blank,
        predicate: t.iri,
        object,
    };
    [
        (
            WriteChange::SetProperty {
                statement: by_reifier,
                predicate: t.iri,
                value: t.literal,
            },
            None,
        ),
        (
            WriteChange::SetProperty {
                statement: by_quad(t.triple),
                predicate: t.iri,
                value: t.literal,
            },
            Some(t.triple),
        ),
        (
            WriteChange::DeleteProperty {
                statement: by_quad(t.literal),
                predicate: t.iri,
                value: Some(t.triple),
            },
            Some(t.triple),
        ),
        (
            WriteChange::DeleteProperty {
                statement: by_reifier,
                predicate: t.iri,
                value: None,
            },
            None,
        ),
    ]
}

/// Each commit-time operation passes with every slot holding what it takes, and is refused naming the first term in a
/// slot that does not take it, a link's included.
#[test]
fn test_each_operation_checks_its_slots() -> TestResult {
    let outcomes = with_terms(|write, t| {
        let table = write.terms();
        let upsert = |links| CommitOperation::UpsertAndLink {
            graph: None,
            predicate: t.iri,
            value: t.literal,
            properties: vec![(t.iri, t.literal)],
            links,
        };
        let swap = |expected| CommitOperation::CompareAndSet {
            graph: None,
            subject: t.iri,
            predicate: t.iri,
            expected,
            new: t.literal,
        };
        [
            (upsert(vec![Link::to_node(t.iri, t.blank)]), None),
            (
                upsert(vec![Link::from_node(t.iri, t.literal)]),
                Some(t.literal),
            ),
            (swap(t.literal), None),
            (swap(t.triple), Some(t.triple)),
        ]
        .into_iter()
        .map(|(operation, expected)| {
            (
                refused_at(check_operation(table, 0, &operation)),
                expected.map(u64::from),
            )
        })
        .collect::<Vec<_>>()
    })?;
    for (index, (got, expected)) in outcomes.into_iter().enumerate() {
        assert_eq!(got, expected, "operation {index}");
    }
    Ok(())
}

/// A slot taking an IRI refuses a skolem IRI no one can hold: another tenant's.
#[test]
fn test_an_iri_nobody_can_hold_is_refused() -> TestResult {
    let mut table = TermTable::new();
    let other =
        format!("{SYSTEM_VOCABULARY}tenant/7/.well-known/genid/000000-00000000000000000000000000");
    let foreign = table.iri(&other);
    let change = WriteChange::Insert {
        graph: None,
        subject: foreign,
        predicate: foreign,
        object: foreign,
    };
    let frame = Write::new(table, Vec::new()).frame(Header::new(CLUSTER, Command::Write))?;
    let view = FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)?;
    let write = WriteView::from_frame(&view)?;
    let refused = check_change(write.terms(), 0, &change)
        .err()
        .context("a refusal")?;
    assert_eq!(refused.detail(), u64::from(foreign));
    Ok(())
}
