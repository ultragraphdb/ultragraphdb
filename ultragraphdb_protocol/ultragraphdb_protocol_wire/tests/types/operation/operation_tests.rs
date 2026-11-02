/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, CommitOperation, Link, Operation, OperationView, RequestTerm, TermTable,
};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::{frame_of, golden};

fn literal(s: &str) -> RequestTerm {
    RequestTerm::Literal {
        lexical: s.to_string(),
        datatype: None,
    }
}

pub fn upsert() -> TestResult<Operation> {
    let mut t = TermTable::new();
    let id = t.iri("http://ex.org/epi.id");
    let x = t.intern(literal("x"))?;
    let label = t.iri("http://ex.org/label");
    let first = t.intern(literal("first"))?;
    let has = t.iri("http://ex.org/HAS_ENTITY");
    let m1 = t.iri("http://ex.org/m1");
    Ok(Operation::new(
        t,
        CommitOperation::UpsertAndLink {
            graph: None,
            predicate: id,
            value: x,
            properties: vec![(label, first)],
            links: vec![Link::to_node(has, m1)],
        },
    ))
}

pub fn counter() -> Operation {
    let mut t = TermTable::new();
    let counters = t.iri("http://ex.org/counters");
    let next = t.iri("http://ex.org/next-id");
    Operation::new(
        t,
        CommitOperation::CounterAdd {
            graph: None,
            subject: counters,
            predicate: next,
            delta: -5,
        },
    )
}

/// The golden insert-if-absent whose value nests 32 deep: <p>, <a>, then entry k + 1 = <<( k <p> k )>>.
pub fn deep() -> TestResult<Operation> {
    let mut t = TermTable::new();
    let p = t.iri("http://ex.org/p");
    let mut deepest = t.iri("http://ex.org/a");
    for _ in 0..32 {
        deepest = t.intern(RequestTerm::Triple {
            subject: deepest,
            predicate: p,
            object: deepest,
        })?;
    }
    Ok(Operation::new(
        t,
        CommitOperation::InsertIfAbsent {
            graph: None,
            predicate: p,
            value: deepest,
            properties: vec![],
        },
    ))
}

#[test]
fn test_operations_build_the_golden_frames() -> TestResult {
    assert_eq!(upsert()?.terms().len(), 6);
    assert!(matches!(
        upsert()?.operation(),
        CommitOperation::UpsertAndLink { .. }
    ));
    assert_eq!(
        upsert()?.frame(golden("operation")?.header())?,
        golden("operation")?.bytes()?
    );
    assert_eq!(
        counter().frame(golden("operation_counter")?.header())?,
        golden("operation_counter")?.bytes()?
    );
    assert_eq!(
        deep()?.frame(golden("operation_deep")?.header())?,
        golden("operation_deep")?.bytes()?
    );
    Ok(())
}

#[test]
fn test_every_kind_round_trips() -> TestResult {
    let mut t = TermTable::new();
    let (a, b, c, d) = (t.iri("a"), t.iri("b"), t.iri("c"), t.iri("d"));
    let operations = [
        CommitOperation::InsertIfAbsent {
            graph: Some(a),
            predicate: b,
            value: c,
            properties: vec![],
        },
        CommitOperation::InsertIfAbsent {
            graph: None,
            predicate: b,
            value: c,
            properties: vec![(a, d), (b, c)],
        },
        CommitOperation::UpsertAndLink {
            graph: None,
            predicate: a,
            value: b,
            properties: vec![],
            links: vec![Link::from_node(c, d), Link::to_node(d, c)],
        },
        CommitOperation::CompareAndSet {
            graph: None,
            subject: a,
            predicate: b,
            expected: c,
            new: d,
        },
        CommitOperation::CounterAdd {
            graph: None,
            subject: a,
            predicate: b,
            delta: i64::MIN,
        },
    ];
    for operation in operations {
        let built = Operation::new(t.clone(), operation.clone());
        let bytes = built.frame(golden("operation")?.header())?;
        let view = OperationView::from_frame(&frame_of(&bytes)?)?;
        assert_eq!(view.operation(), operation);
        assert_eq!(view.to_body()?, built);
    }
    Ok(())
}
