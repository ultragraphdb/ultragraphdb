/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphName, Literal};
use ultragraphdb_protocol_wire::CommitOperation;

use crate::types::write::write_tests::ex;

#[test]
fn test_operations_compare_by_value() {
    let add = |delta| CommitOperation::CounterAdd {
        graph: GraphName::DefaultGraph,
        subject: ex("c").into(),
        predicate: ex("n"),
        delta,
    };
    assert_eq!(add(1), add(1));
    assert_ne!(add(1), add(-1));
    let cas = CommitOperation::CompareAndSet {
        graph: GraphName::DefaultGraph,
        subject: ex("a").into(),
        predicate: ex("status"),
        expected: Literal::new_simple_literal("open").into(),
        new: Literal::new_simple_literal("closed").into(),
    };
    assert_ne!(cas, add(0));
}
