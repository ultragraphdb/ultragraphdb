/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{CommitOperation, Link};

#[test]
fn test_operations_compare_by_value() {
    let add = |delta| CommitOperation::CounterAdd {
        graph: None,
        subject: 0,
        predicate: 1,
        delta,
    };
    assert_eq!(add(1), add(1));
    assert_ne!(add(1), add(-1));
    let cas = CommitOperation::CompareAndSet {
        graph: Some(0),
        subject: 1,
        predicate: 2,
        expected: 3,
        new: 4,
    };
    assert_ne!(cas, add(0));
    let upsert = CommitOperation::UpsertAndLink {
        graph: None,
        predicate: 0,
        value: 1,
        properties: vec![],
        links: vec![Link::from_node(2, 3)],
    };
    assert_ne!(
        upsert,
        CommitOperation::InsertIfAbsent {
            graph: None,
            predicate: 0,
            value: 1,
            properties: vec![]
        }
    );
}
