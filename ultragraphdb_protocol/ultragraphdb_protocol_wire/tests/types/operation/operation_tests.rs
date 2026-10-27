/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphName, Literal};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, CommitOperation, Link, Operation, OperationView,
};

use crate::types::write::write_tests::ex;
use crate::utils::golden_bodies::{frame_of, golden};

pub fn upsert() -> CommitOperation {
    CommitOperation::UpsertAndLink {
        graph: GraphName::DefaultGraph,
        predicate: ex("epi.id"),
        value: Literal::new_simple_literal("x").into(),
        properties: vec![(ex("label"), Literal::new_simple_literal("first").into())],
        links: vec![Link::to_node(ex("HAS_ENTITY"), ex("m1").into())],
    }
}

pub fn counter() -> CommitOperation {
    CommitOperation::CounterAdd {
        graph: GraphName::DefaultGraph,
        subject: ex("counters").into(),
        predicate: ex("next-id"),
        delta: -5,
    }
}

#[test]
fn test_operations_build_the_golden_frames() {
    assert_eq!(Operation::new(upsert()).operation(), &upsert());
    assert_eq!(
        Operation::new(upsert())
            .frame(golden("operation").header())
            .unwrap(),
        golden("operation").bytes()
    );
    assert_eq!(
        Operation::new(counter())
            .frame(golden("operation_counter").header())
            .unwrap(),
        golden("operation_counter").bytes()
    );
}

#[test]
fn test_every_kind_round_trips() {
    let operations = [
        CommitOperation::InsertIfAbsent {
            graph: ex("g").into(),
            predicate: ex("epi.id"),
            value: Literal::new_simple_literal("y").into(),
            properties: vec![],
        },
        CommitOperation::InsertIfAbsent {
            graph: GraphName::DefaultGraph,
            predicate: ex("epi.id"),
            value: Literal::new_simple_literal("y").into(),
            properties: vec![
                (ex("label"), Literal::new_simple_literal("second").into()),
                (ex("rank"), Literal::from(3).into()),
            ],
        },
        CommitOperation::UpsertAndLink {
            graph: GraphName::DefaultGraph,
            predicate: ex("epi.id"),
            value: Literal::new_simple_literal("z").into(),
            properties: vec![],
            links: vec![
                Link::from_node(ex("p"), ex("o").into()),
                Link::to_node(ex("q"), ex("s").into()),
            ],
        },
        CommitOperation::CompareAndSet {
            graph: GraphName::DefaultGraph,
            subject: ex("a").into(),
            predicate: ex("status"),
            expected: Literal::new_simple_literal("open").into(),
            new: Literal::new_simple_literal("closed").into(),
        },
        CommitOperation::CounterAdd {
            graph: GraphName::DefaultGraph,
            subject: ex("c").into(),
            predicate: ex("n"),
            delta: i64::MIN,
        },
        counter(),
        upsert(),
    ];
    for operation in operations {
        let bytes = Operation::new(operation.clone())
            .frame(golden("operation").header())
            .unwrap();
        let view = OperationView::from_frame(&frame_of(&bytes)).unwrap();
        assert_eq!(view.to_body().unwrap().operation(), &operation);
    }
}
