/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{Direction, MAX_TERM_DEPTH, RequestTerm, TermTable};

fn iri(s: &str) -> RequestTerm {
    RequestTerm::Iri(s.to_string())
}

#[test]
fn test_a_term_is_interned_once_in_order_of_first_use() {
    let mut table = TermTable::new();
    assert!(table.is_empty());
    assert_eq!(table.intern(iri("http://ex.org/a")), Ok(0));
    assert_eq!(
        table.intern(RequestTerm::BlankNode("b1".to_string())),
        Ok(1)
    );
    assert_eq!(table.iri("http://ex.org/a"), 0);
    assert_eq!(table.iri("http://ex.org/c"), 2);
    // A blank node and an IRI of the same text are two terms.
    assert_eq!(
        table.intern(RequestTerm::BlankNode("http://ex.org/a".to_string())),
        Ok(3)
    );
    assert_eq!(table.len(), 4);
    assert_eq!(
        table.get(1),
        Some(&RequestTerm::BlankNode("b1".to_string()))
    );
    assert_eq!(table.get(4), None);
    assert_eq!(table.terms().len(), 4);
}

#[test]
fn test_literals_take_one_form_each() {
    let mut table = TermTable::new();
    let plain = RequestTerm::Literal {
        lexical: "x".to_string(),
        datatype: None,
    };
    let typed = RequestTerm::Literal {
        lexical: "x".to_string(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".to_string()),
    };
    let tagged = |direction| RequestTerm::LanguageLiteral {
        lexical: "x".to_string(),
        language: "en".to_string(),
        direction,
    };
    assert_eq!(table.intern(plain.clone()), Ok(0));
    assert_eq!(table.intern(typed), Ok(1));
    assert_eq!(table.intern(tagged(None)), Ok(2));
    assert_eq!(table.intern(tagged(Some(Direction::Ltr))), Ok(3));
    assert_eq!(table.intern(tagged(Some(Direction::Rtl))), Ok(4));
    assert_eq!(table.intern(plain), Ok(0));
    // xsd:string is the literal without a datatype; stated, it would be a second encoding of it.
    let stated = RequestTerm::Literal {
        lexical: "x".to_string(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#string".to_string()),
    };
    assert_eq!(
        table.intern(stated).unwrap_err().0,
        ProtocolErrorEnum::BadTerm {
            command: 0,
            section: 0,
            at: 5
        }
    );
    let untagged = RequestTerm::LanguageLiteral {
        lexical: "x".to_string(),
        language: String::new(),
        direction: None,
    };
    assert_eq!(
        table.intern(untagged).unwrap_err().0,
        ProtocolErrorEnum::BadTerm {
            command: 0,
            section: 0,
            at: 5
        }
    );
    assert_eq!(table.len(), 5);
}

#[test]
fn test_a_triple_term_names_entries_already_in_the_table() {
    let mut table = TermTable::new();
    let (a, knows, b) = (table.iri("a"), table.iri("knows"), table.iri("b"));
    let triple = RequestTerm::Triple {
        subject: a,
        predicate: knows,
        object: b,
    };
    assert_eq!(table.intern(triple.clone()), Ok(3));
    assert_eq!(table.intern(triple), Ok(3));
    let forward = RequestTerm::Triple {
        subject: a,
        predicate: knows,
        object: 4,
    };
    assert_eq!(
        table.intern(forward).unwrap_err().0,
        ProtocolErrorEnum::IndexOutOfRange {
            command: 0,
            section: 0,
            at: 4,
            index: 4,
            limit: 4
        }
    );
}

/// A table of `<a>` and the chain of triple terms over it to `depth`, the deepest last: entry k + 1 is
/// `<<( a a entry k )>>`, so only the object deepens.
fn chain(depth: usize) -> Result<TermTable, ultragraphdb_error_protocol::ProtocolError> {
    let mut table = TermTable::new();
    let a = table.iri("a");
    let mut deepest = a;
    for _ in 0..depth {
        deepest = table.intern(RequestTerm::Triple {
            subject: a,
            predicate: a,
            object: deepest,
        })?;
    }
    Ok(table)
}

#[test]
fn test_triple_terms_nest_at_most_the_maximum() {
    assert_eq!(MAX_TERM_DEPTH, 32);
    assert_eq!(chain(MAX_TERM_DEPTH).unwrap().len(), MAX_TERM_DEPTH + 1);
    assert_eq!(
        chain(MAX_TERM_DEPTH + 1).unwrap_err().0,
        ProtocolErrorEnum::TermTooDeep {
            command: 0,
            section: 0,
            at: 33,
            depth: 33,
            limit: 32
        }
    );
}
