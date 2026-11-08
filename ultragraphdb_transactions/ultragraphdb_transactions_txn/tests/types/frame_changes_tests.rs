/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{BaseDirection, GraphNameRef, NamedOrBlankNodeRef, TermRef, vocab::rdf};
use ultragraphdb_core_mapping::{ChangeRef, Changes, StatementView};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, Declared, DeclaredPredicate, Direction, FrameView, Header,
    MAX_FRAME_SIZE, RequestTerm, StatementTarget, TermTable, Write, WriteChange, WriteView,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
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
            assert_eq!(changes.tenant(), 9);
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

/// The changes the Write of `changes` over `table` reads as, owned so they outlive the frame: each one's `Debug`.
fn read_as(table: TermTable, changes: Vec<WriteChange>) -> TestResult<Vec<String>> {
    reading(table, changes, |read| {
        read.map(|frame| {
            frame
                .changes()
                .map(|change| format!("{change:?}"))
                .collect::<Vec<_>>()
        })
    })?
    .map_err(TestError::new)
}

/// A language-tagged literal whose tag RDF already holds lowercase is borrowed as sent, its direction with it; an
/// uppercase tag without a direction is lowered too.
#[test]
fn test_a_language_literal_reads_with_its_tag_and_direction() -> TestResult {
    let mut table = TermTable::new();
    let a = table.iri(IRI);
    let mut literals = Vec::new();
    for (language, direction) in [("en", None), ("en", Some(Direction::Ltr)), ("EN", None)] {
        literals.push(table.intern(RequestTerm::LanguageLiteral {
            lexical: "x".to_string(),
            language: language.to_string(),
            direction,
        })?);
    }
    let changes = literals
        .iter()
        .map(|literal| insert(a, a, *literal))
        .collect();
    let read = reading(table, changes, |read| {
        read.map(|frame| {
            frame
                .changes()
                .filter_map(|change| {
                    if let ChangeRef::Insert {
                        object: TermRef::Literal(literal),
                        ..
                    } = change
                    {
                        Some((literal.language().map(str::to_string), literal.direction()))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
    })?
    .map_err(TestError::new)?;
    let en = Some("en".to_string());
    assert_eq!(
        read,
        [
            (en.clone(), None),
            (en.clone(), Some(BaseDirection::Ltr)),
            (en, None)
        ]
    );
    Ok(())
}

/// A triple term's subject may be a blank node; a literal subject or a predicate that is no IRI is refused.
#[test]
fn test_a_triple_term_s_subject_and_predicate_are_checked() -> TestResult {
    let mut table = TermTable::new();
    let a = table.iri(IRI);
    let blank = table.intern(RequestTerm::BlankNode("b0".to_string()))?;
    let literal = table.intern(RequestTerm::Literal {
        lexical: "x".to_string(),
        datatype: None,
    })?;
    let held = table.intern(RequestTerm::Triple {
        subject: blank,
        predicate: a,
        object: a,
    })?;
    assert_eq!(read_as(table.clone(), vec![insert(a, a, held)])?.len(), 1);
    for (subject, predicate) in [(literal, a), (a, literal)] {
        let mut table = table.clone();
        let triple = table.intern(RequestTerm::Triple {
            subject,
            predicate,
            object: a,
        })?;
        let refused = reading(table, vec![insert(a, a, triple)], |read| read.err())?;
        assert!(refused.context("a refusal")?.contains("term"));
    }
    Ok(())
}

/// A triple term the terms passed over was never held, so reading it is refused naming the term.
#[test]
fn test_a_triple_term_not_held_is_refused() -> TestResult {
    let mut table = TermTable::new();
    let a = table.iri(IRI);
    let triple = table.intern(RequestTerm::Triple {
        subject: a,
        predicate: a,
        object: a,
    })?;
    let frame = Write::new(table, vec![insert(a, a, triple)])
        .frame(Header::new(CLUSTER, Command::Write).with_stream(1))?;
    let view = FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)?;
    let write = WriteView::from_frame(&view)?;
    let none = FrameTerms::default();
    let refused = FrameChanges::new(9, &write, &none)
        .err()
        .context("a refusal")?;
    assert!(refused.to_string().contains("term"), "{refused}");
    Ok(())
}

/// Every kind of change but a declaration reads its terms in their places: a blank graph, a statement by its reifier or
/// by its quad, a property's value or none.
#[test]
fn test_every_change_reads_its_terms() -> TestResult {
    let mut table = TermTable::new();
    let a = table.iri(IRI);
    let graph = table.intern(RequestTerm::BlankNode("g".to_string()))?;
    let by_reifier = StatementTarget::Reifier {
        graph: Some(graph),
        reifier: a,
    };
    let by_quad = StatementTarget::Triple {
        graph: None,
        subject: a,
        predicate: a,
        object: a,
    };
    let changes = vec![
        WriteChange::DeletePattern {
            graph: Some(graph),
            subject: a,
            predicate: a,
        },
        WriteChange::SetProperty {
            statement: by_reifier,
            predicate: a,
            value: a,
        },
        WriteChange::DeleteProperty {
            statement: by_quad,
            predicate: a,
            value: None,
        },
        WriteChange::DeleteProperty {
            statement: by_reifier,
            predicate: a,
            value: Some(a),
        },
    ];
    let count = changes.len();
    let read = reading(table, changes, |read| {
        read.map(|frame| {
            frame
                .changes()
                .map(|change| reads_in_place(&change))
                .collect::<Vec<_>>()
        })
    })?
    .map_err(TestError::new)?;
    assert_eq!(read, vec![true; count]);
    Ok(())
}

/// Whether `change` reads its terms in the places [`test_every_change_reads_its_terms`] writes them.
fn reads_in_place(change: &ChangeRef<'_>) -> bool {
    matches!(
        change,
        ChangeRef::DeletePattern {
            graph: GraphNameRef::BlankNode(_),
            ..
        } | ChangeRef::SetProperty {
            statement: StatementView::Reifier {
                graph: GraphNameRef::BlankNode(_),
                ..
            },
            ..
        } | ChangeRef::DeleteProperty {
            statement: StatementView::Triple {
                subject: NamedOrBlankNodeRef::NamedNode(_),
                ..
            },
            value: None,
            ..
        } | ChangeRef::DeleteProperty {
            statement: StatementView::Reifier { .. },
            value: Some(_),
            ..
        }
    )
}

/// A declaration commits through the metadata group, so a Write carrying one is refused naming it.
#[test]
fn test_a_declaration_is_refused() -> TestResult {
    let table = TermTable::new();
    let declared = WriteChange::Declare(Declared::Predicate(DeclaredPredicate::new(IRI)));
    let refused = reading(table, vec![declared], |read| read.err())?;
    assert!(refused.context("a refusal")?.contains("declaration"));
    Ok(())
}
