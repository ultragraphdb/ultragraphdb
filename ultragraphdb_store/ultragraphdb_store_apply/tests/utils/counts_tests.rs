/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Tests of the port of Dgraph v25.5.0, posting/index.go (countAfterMutation, shouldAddCountEdge). Changed for
 * ultragraphdb: written for the port; Dgraph tests both only through its rebuild tests.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_proto_pb::pb::directed_edge::Op;
use ultragraphdb_store_apply::{count_after_mutation, should_add_count_edge};

#[test]
fn test_count_after_mutation() {
    // A scalar counts one when set, none when its value is deleted.
    assert_eq!(count_after_mutation(0, false, Op::Set, true), 1);
    assert_eq!(count_after_mutation(1, true, Op::Set, true), 1);
    assert_eq!(count_after_mutation(1, true, Op::Del, true), 0);
    assert_eq!(count_after_mutation(1, false, Op::Del, true), 1);
    // A list counts its postings.
    assert_eq!(count_after_mutation(2, false, Op::Set, false), 3);
    assert_eq!(count_after_mutation(2, true, Op::Set, false), 2);
    assert_eq!(count_after_mutation(2, true, Op::Del, false), 1);
    assert_eq!(count_after_mutation(2, false, Op::Del, false), 2);
}

#[test]
fn test_should_add_count_edge() {
    let mut op = Op::Set;
    assert!(should_add_count_edge(true, &mut op));
    assert_eq!(op, Op::Ovr, "a set of a posting held is an overwrite");
    let mut op = Op::Del;
    assert!(should_add_count_edge(true, &mut op));
    assert_eq!(op, Op::Del);
    assert!(should_add_count_edge(false, &mut Op::Set));
    assert!(!should_add_count_edge(false, &mut Op::Del));
}
