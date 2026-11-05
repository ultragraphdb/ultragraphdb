/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, SessionToken};
use ultragraphdb_protocol_wire::{
    BodyBuilder, CommitOperation, ConditionalWrite, Link, RequestTerm, TermTable, WriteChange,
};
use ultragraphdb_tool_bdd::TestResult;

use crate::types::commit::commit_tests::session;
use crate::utils::golden_bodies::golden;

const EX: &str = "http://ex.org/";

fn ex(name: &str) -> String {
    format!("{EX}{name}")
}

fn literal(terms: &mut TermTable, lexical: &str) -> TestResult<u32> {
    Ok(terms.intern(RequestTerm::Literal {
        lexical: lexical.to_string(),
        datatype: None,
    })?)
}

/// The golden ConditionalWrite's parts, its terms interned in generate_bodies.py's order of first use.
fn golden_parts() -> TestResult<(TermTable, Vec<WriteChange>, Vec<CommitOperation>)> {
    let mut t = TermTable::new();
    let a = t.iri(&ex("a"));
    let knows = t.iri(&ex("knows"));
    let b = t.iri(&ex("b"));
    let id = t.iri(&ex("epi.id"));
    let y = literal(&mut t, "y")?;
    let label = t.iri(&ex("label"));
    let second = literal(&mut t, "second")?;
    let g = t.iri(&ex("g"));
    let x = literal(&mut t, "x")?;
    let first = literal(&mut t, "first")?;
    let has = t.iri(&ex("HAS_ENTITY"));
    let m1 = t.iri(&ex("m1"));
    let state = t.iri(&ex("state"));
    let old = literal(&mut t, "old")?;
    let new = literal(&mut t, "new")?;
    let counters = t.iri(&ex("counters"));
    let next = t.iri(&ex("next-id"));
    let changes = vec![WriteChange::Insert {
        graph: None,
        subject: a,
        predicate: knows,
        object: b,
    }];
    let operations = vec![
        CommitOperation::InsertIfAbsent {
            graph: None,
            predicate: id,
            value: y,
            properties: vec![(label, second)],
        },
        CommitOperation::UpsertAndLink {
            graph: Some(g),
            predicate: id,
            value: x,
            properties: vec![(label, first)],
            links: vec![Link::to_node(has, m1)],
        },
        CommitOperation::CompareAndSet {
            graph: None,
            subject: a,
            predicate: state,
            expected: old,
            new,
        },
        CommitOperation::CounterAdd {
            graph: None,
            subject: counters,
            predicate: next,
            delta: -5,
        },
    ];
    Ok((t, changes, operations))
}

/// The golden ConditionalWrite: through the session's request 10 in namespace 3, holding the token of partition 2's
/// commit at position 42 in epoch 31.
///
/// # Errors
///
/// When a term or the epoch is refused.
pub fn golden_conditional_write() -> TestResult<ConditionalWrite> {
    let (terms, changes, operations) = golden_parts()?;
    Ok(ConditionalWrite::new(3, terms, changes, operations)
        .with_session(session(), 10)
        .with_token(SessionToken::new(2, 42, Epoch::new(31)?)))
}

/// The golden ConditionalWrite through no session, holding no token and no change: one counter add in namespace 4.
pub fn golden_operation_only() -> ConditionalWrite {
    let mut t = TermTable::new();
    let (counters, next) = (t.iri(&ex("counters")), t.iri(&ex("next-id")));
    ConditionalWrite::new(
        4,
        t,
        Vec::new(),
        vec![CommitOperation::CounterAdd {
            graph: None,
            subject: counters,
            predicate: next,
            delta: 1,
        }],
    )
}

#[test]
fn test_a_conditional_write_builds_the_golden_frames() -> TestResult {
    assert_eq!(
        golden_conditional_write()?.frame(golden("conditional_write")?.header())?,
        golden("conditional_write")?.bytes()?
    );
    let name = "conditional_write_operation_only";
    assert_eq!(
        golden_operation_only().frame(golden(name)?.header())?,
        golden(name)?.bytes()?
    );
    Ok(())
}

#[test]
fn test_a_conditional_write_holds_what_it_was_given() -> TestResult {
    let (terms, changes, operations) = golden_parts()?;
    let token = SessionToken::new(2, 42, Epoch::new(31)?);
    let write = ConditionalWrite::new(3, terms.clone(), changes.clone(), operations.clone())
        .with_session(session(), 10)
        .with_token(token.clone());
    assert_eq!(write.session(), session());
    assert_eq!(write.request(), 10);
    assert_eq!(write.namespace(), 3);
    assert_eq!(write.token(), Some(&token));
    assert_eq!(write.terms(), &terms);
    assert_eq!(write.changes(), &changes[..]);
    assert_eq!(write.operations(), &operations[..]);
    Ok(())
}

#[test]
fn test_a_new_conditional_write_goes_through_no_session_and_holds_no_token() {
    let write = golden_operation_only();
    assert_eq!(write.session(), [0; 16]);
    assert_eq!(write.request(), 0);
    assert_eq!(write.token(), None);
}
