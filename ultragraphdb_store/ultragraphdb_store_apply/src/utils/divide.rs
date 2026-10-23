/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/x.go (DivideAndRule) and worker/draft.go (the batches of applyMutations). Changed
 * for ultragraphdb: translated to Rust; a batch holds whole predicates, where Dgraph's batches of `width` edges keep
 * only a subject's edges of a predicate together: the batches of a request are applied in forks of it, which must
 * hold disjoint lists, and every list of a predicate, its index, reverse and count keys included, is the
 * predicate's own.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::ops::Range;

use ultragraphdb_proto_pb::pb::DirectedEdge;

/// Dgraph's `DivideAndRule`: how many goroutines apply `num` edges, from 64 halving while each would take fewer than
/// 256, and how many each takes.
pub fn divide_and_rule(num: usize) -> (usize, usize) {
    let mut num_go = 64usize;
    while num_go >= 1 {
        let width = num.div_ceil(num_go);
        if num_go == 1 || width >= 256 {
            return (num_go, width);
        }
        num_go /= 2;
    }
    (num_go, 0)
}

/// The batches `edges`, sorted by predicate and subject, are applied in: all of them in one when Dgraph would run one
/// goroutine, otherwise one batch per predicate.
pub fn edge_batches(edges: &[DirectedEdge]) -> Vec<Range<usize>> {
    if edges.is_empty() {
        return Vec::new();
    }
    let (num_go, _) = divide_and_rule(edges.len());
    if num_go == 1 {
        let all = 0..edges.len();
        return vec![all];
    }
    let mut batches = Vec::new();
    let mut start = 0;
    for i in 1..=edges.len() {
        if i == edges.len() || edges[i].attr != edges[start].attr {
            batches.push(start..i);
            start = i;
        }
    }
    batches
}
