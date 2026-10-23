/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/index.go (countParams, shouldAddCountEdge, countAfterMutation). Changed for
 * ultragraphdb: translated to Rust; the edge's operation is passed alone.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_proto_pb::pb::directed_edge::Op;

/// What a count index update moves: the subject from the count key of `before` to that of `after`. Dgraph's
/// `countParams`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CountParams {
    pub attr: String,
    pub before: i64,
    pub after: i64,
    pub entity: u64,
    pub reverse: bool,
}

/// Dgraph's `shouldAddCountEdge`: whether a mutation of a list with a count index is applied; a set of a posting
/// the list holds becomes an overwrite, which leaves the count.
pub fn should_add_count_edge(found: bool, op: &mut Op) -> bool {
    if found {
        if *op != Op::Del {
            *op = Op::Ovr;
        }
        true
    } else {
        *op != Op::Del
    }
}

/// Dgraph's `countAfterMutation`: a list's length after a mutation whose posting was `found` before it. A scalar
/// counts one whenever set, none once its value is deleted.
pub fn count_after_mutation(before: i64, found: bool, op: Op, count_one: bool) -> i64 {
    if count_one {
        return match op {
            Op::Set => 1,
            Op::Del if found => 0,
            _ => before,
        };
    }
    match (found, op) {
        (false, Op::Set | Op::Ovr) => before + 1,
        (true, Op::Del) => before - 1,
        _ => before,
    }
}
