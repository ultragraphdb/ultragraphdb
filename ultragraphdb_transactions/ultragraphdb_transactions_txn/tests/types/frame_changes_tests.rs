/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{TermRef, vocab::rdf};
use ultragraphdb_core_mapping::{ChangeRef, Changes};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, Direction, FrameView, Header, MAX_FRAME_SIZE, RequestTerm,
    TermTable, Write, WriteChange, WriteView,
};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transactions_txn::{FrameChanges, FrameTerms};

const CLUSTER: u64 = 7;
const IRI: &str = "http://ex.org/a";

/// What `check` makes of the Write of `changes` over `table`, read from its frame.
fn reading<T>(
    table: TermTable,
    changes: Vec<WriteChange>,
    check: impl FnOnce(Result<FrameChanges<'_>, String>) -> T,
) -> TestResult<T> {
    let frame =
        Write::new(table, changes).frame(Header::new(CLUSTER, Command::Write).with_stream(1))?;
    let view = FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)?;
    let write = WriteView::from_frame(&view)?;
    let terms = FrameTerms::of(write.terms());
    let terms = match terms {
        Ok(terms) => terms,
        Err(rule) => return Ok(check(Err(rule.to_string()))),
    };
    let read = FrameChanges::new(9, &write, &terms).map_err(|rule| rule.to_string());
    Ok(check(read))
}

fn insert(subject: u32, predicate: u32, object: u32) -> WriteChange {
    WriteChange::Insert {
        graph: None,
        subject,
        predicate,
        object,
    }
}

/// An IRI that does not parse, wherever it stands, refuses the frame by naming the term.
#[test]
fn test_a_malformed_term_is_refused() -> TestResult {
    for term in [
        RequestTerm::Iri("not an iri".to_string()),
        RequestTerm::BlankNode("not a label".to_string()),
        RequestTerm::Literal {
            lexical: "x".to_string(),
            datatype: Some("not an iri".to_string()),
        },
        RequestTerm::LanguageLiteral {
            lexical: "x".to_string(),
            language: "!!".to_string(),
            direction: None,
        },
    ] {
        let mut table = TermTable::new();
        let a = table.iri(IRI);
        let bad = table.intern(term)?;
        let refused = reading(table, vec![insert(a, a, bad)], |read| read.err())?;
        assert!(refused.context("a refusal")?.contains("term"));
    }
    Ok(())
}

/// A term of the wrong kind for its place is refused naming the place; a term's place decides its kind.
#[test]
fn test_a_term_of_the_wrong_kind_is_refused_naming_its_place() -> TestResult {
    let mut table = TermTable::new();
    let a = table.iri(IRI);
    let literal = table.intern(RequestTerm::Literal {
        lexical: "x".to_string(),
        datatype: None,
    })?;
    for (change, place) in [
        (insert(literal, a, a), "subject"),
        (insert(a, literal, a), "predicate"),
        (
            WriteChange::Insert {
                graph: Some(literal),
                subject: a,
                predicate: a,
                object: a,
            },
            "graph",
        ),
        (
            WriteChange::DeleteStatement {
                graph: None,
                reifier: literal,
            },
            "reifier",
        ),
    ] {
        let refused = reading(table.clone(), vec![change], |read| read.err())?;
        assert!(refused.context("a refusal")?.contains(place), "{place}");
    }
    Ok(())
}

/// A tag not in RDF's lowercase is held lowercased, with its direction; a lowercase one is borrowed as sent.
#[test]
fn test_a_language_tag_reads_as_rdf_holds_it() -> TestResult {
    let mut table = TermTable::new();
    let a = table.iri(IRI);
    let upper = table.intern(RequestTerm::LanguageLiteral {
        lexical: "x".to_string(),
        language: "EN-gb".to_string(),
        direction: Some(Direction::Rtl),
    })?;
    let languages = reading(table, vec![insert(a, a, upper)], |read| {
        read.map(|changes| {
            assert_eq!(changes.namespace(), 9);
            changes
                .changes()
                .filter_map(|change| {
                    if let ChangeRef::Insert {
                        object: TermRef::Literal(literal),
                        ..
                    } = change
                    {
                        literal.language().map(str::to_string)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
    })?;
    assert_eq!(
        languages.map_err(ultragraphdb_tool_bdd::TestError::new)?,
        ["en-gb"]
    );
    Ok(())
}

/// A triple term is held as RDF holds it; one whose subject is a literal is refused.
#[test]
fn test_a_triple_term_is_held_and_checked() -> TestResult {
    let mut table = TermTable::new();
    let a = table.iri(IRI);
    let typed = table.iri(rdf::TYPE.as_str());
    let triple = table.intern(RequestTerm::Triple {
        subject: a,
        predicate: typed,
        object: a,
    })?;
    let held = reading(table, vec![insert(a, a, triple)], |read| {
        read.map(|changes| {
            changes.changes().any(|change| {
                matches!(
                    change,
                    ChangeRef::Insert {
                        object: TermRef::Triple(_),
                        ..
                    }
                )
            })
        })
    })?;
    assert_eq!(held, Ok(true));
    Ok(())
}
