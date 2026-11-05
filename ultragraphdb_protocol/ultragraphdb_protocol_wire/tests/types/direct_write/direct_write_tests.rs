/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, SessionToken};
use ultragraphdb_protocol_wire::{BodyBuilder, DirectWrite, RequestTerm, TermTable, WriteChange};
use ultragraphdb_tool_bdd::TestResult;

use crate::types::commit::commit_tests::session;
use crate::utils::golden_bodies::golden;

const EX: &str = "http://ex.org/";

fn ex(name: &str) -> String {
    format!("{EX}{name}")
}

/// The golden DirectWrite's changes, its terms interned in generate_bodies.py's order of first use.
fn golden_changes() -> TestResult<(TermTable, Vec<WriteChange>)> {
    let mut t = TermTable::new();
    let a = t.iri(&ex("a"));
    let knows = t.iri(&ex("knows"));
    let b = t.iri(&ex("b"));
    let c = t.iri(&ex("c"));
    let since = t.iri(&ex("since"));
    let year = t.intern(RequestTerm::Literal {
        lexical: "2020".to_string(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#gYear".to_string()),
    })?;
    let g = t.iri(&ex("g"));
    let b1 = t.intern(RequestTerm::BlankNode("b1".to_string()))?;
    let nick = t.iri(&ex("nick"));
    let x = t.intern(RequestTerm::LanguageLiteral {
        lexical: "x".to_string(),
        language: "en".to_string(),
        direction: None,
    })?;
    Ok((
        t,
        vec![
            WriteChange::Insert {
                graph: None,
                subject: a,
                predicate: knows,
                object: b,
            },
            WriteChange::InsertEdge {
                graph: None,
                subject: a,
                predicate: knows,
                object: c,
                properties: vec![(since, year)],
            },
            WriteChange::Delete {
                graph: Some(g),
                subject: b1,
                predicate: nick,
                object: x,
            },
        ],
    ))
}

/// The golden DirectWrite: through the session's request 9 in tenant 3, holding the token of partition 2's commit
/// at position 41 in epoch 30.
///
/// # Errors
///
/// When a term or the epoch is refused.
pub fn golden_direct_write() -> TestResult<DirectWrite> {
    let (terms, changes) = golden_changes()?;
    Ok(DirectWrite::new(3, terms, changes)
        .with_session(session(), 9)
        .with_token(SessionToken::new(2, 41, Epoch::new(30)?)))
}

#[test]
fn test_a_direct_write_builds_the_golden_frame() -> TestResult {
    assert_eq!(
        golden_direct_write()?.frame(golden("direct_write")?.header())?,
        golden("direct_write")?.bytes()?
    );
    Ok(())
}

#[test]
fn test_a_direct_write_holds_what_it_was_given() -> TestResult {
    let (terms, changes) = golden_changes()?;
    let token = SessionToken::new(2, 41, Epoch::new(30)?);
    let write = DirectWrite::new(3, terms.clone(), changes.clone())
        .with_session(session(), 9)
        .with_token(token.clone());
    assert_eq!(write.session(), session());
    assert_eq!(write.request(), 9);
    assert_eq!(write.tenant(), 3);
    assert_eq!(write.token(), Some(&token));
    assert_eq!(write.terms(), &terms);
    assert_eq!(write.changes(), &changes[..]);
    Ok(())
}

#[test]
fn test_a_new_direct_write_goes_through_no_session_and_holds_no_token() {
    let write = DirectWrite::new(4, TermTable::new(), Vec::new());
    assert_eq!(write.session(), [0; 16]);
    assert_eq!(write.request(), 0);
    assert_eq!(write.token(), None);
}

#[test]
fn test_a_relayed_write_builds_the_golden_frame() -> TestResult {
    let golden_relayed = golden("direct_write_relayed")?;
    let relayed = golden_direct_write()?.with_principal(9);
    assert_eq!(relayed.principal(), 9);
    assert_eq!(
        relayed.frame(golden_relayed.header())?,
        golden_relayed.bytes()?
    );
    Ok(())
}
