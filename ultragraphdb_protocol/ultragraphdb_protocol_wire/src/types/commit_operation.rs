/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::link::Link;

/// An operation a transaction asks the commit step to decide in commit order (D4). Its terms are term-table indices
/// of the Operation, its graph `None` for the default graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CommitOperation {
    /// The node holding `value` of the unique `predicate` in `graph`, created with `properties` if none does.
    InsertIfAbsent {
        graph: Option<u32>,
        predicate: u32,
        value: u32,
        properties: Vec<(u32, u32)>,
    },
    /// As insert-if-absent, and in the same step the `links` between the node and other nodes.
    UpsertAndLink {
        graph: Option<u32>,
        predicate: u32,
        value: u32,
        properties: Vec<(u32, u32)>,
        links: Vec<Link>,
    },
    /// `subject`'s single value of `predicate` replaced by `new` when it is `expected`; else the transaction is refused
    /// with the value found.
    CompareAndSet {
        graph: Option<u32>,
        subject: u32,
        predicate: u32,
        expected: u32,
        new: u32,
    },
    /// `delta` added to `subject`'s integer counter `predicate`, reading nothing.
    CounterAdd {
        graph: Option<u32>,
        subject: u32,
        predicate: u32,
        delta: i64,
    },
}
