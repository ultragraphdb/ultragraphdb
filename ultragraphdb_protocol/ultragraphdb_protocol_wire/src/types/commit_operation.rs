/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphName, NamedNode, NamedOrBlankNode, Term};

use crate::types::link::Link;

/// An operation a transaction asks the commit step to decide in commit order (D4).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CommitOperation {
    /// The node holding `value` of the unique `predicate` in `graph`, created with `properties` if none does.
    InsertIfAbsent {
        graph: GraphName,
        predicate: NamedNode,
        value: Term,
        properties: Vec<(NamedNode, Term)>,
    },
    /// As insert-if-absent, and in the same step the `links` between the node and other nodes.
    UpsertAndLink {
        graph: GraphName,
        predicate: NamedNode,
        value: Term,
        properties: Vec<(NamedNode, Term)>,
        links: Vec<Link>,
    },
    /// `subject`'s single value of `predicate` replaced by `new` when it is `expected`; else the transaction is refused
    /// with the value found.
    CompareAndSet {
        graph: GraphName,
        subject: NamedOrBlankNode,
        predicate: NamedNode,
        expected: Term,
        new: Term,
    },
    /// `delta` added to `subject`'s integer counter `predicate`, reading nothing.
    CounterAdd {
        graph: GraphName,
        subject: NamedOrBlankNode,
        predicate: NamedNode,
        delta: i64,
    },
}
