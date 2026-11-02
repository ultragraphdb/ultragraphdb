/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphName, NamedNode, NamedOrBlankNode, Term};

use crate::types::operation_link::OperationLink;

/// A commit-time operation (D4, 7.5): decided by the commit step in commit order, after the transaction's other
/// checks, against the store at the watermark and every decided but unapplied round's writes
/// ([`crate::ChangesetApplier::operate`]). Its writes go through index maintenance as a changeset's do; the transaction's
/// other writes do not name its result, which the reply carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    /// The node holding `value` of the unique `predicate` in `graph`, created with `properties` if none does.
    InsertIfAbsent {
        /// The graph the node is looked up and created in.
        graph: GraphName,
        /// The unique predicate whose value names the node.
        predicate: NamedNode,
        /// The value that names the node.
        value: Term,
        /// The properties a node created here is given.
        properties: Vec<(NamedNode, Term)>,
    },
    /// As insert-if-absent, and in the same step `links`, blind edges between the node and other nodes.
    UpsertAndLink {
        /// The graph the node is looked up and created in.
        graph: GraphName,
        /// The unique predicate whose value names the node.
        predicate: NamedNode,
        /// The value that names the node.
        value: Term,
        /// The properties a node created here is given.
        properties: Vec<(NamedNode, Term)>,
        /// The edges written between the node and other nodes.
        links: Vec<OperationLink>,
    },
    /// `subject`'s value of `predicate` replaced by `new` when it is `expected`; else the transaction is refused with
    /// the value found.
    CompareAndSet {
        /// The graph the value is held in.
        graph: GraphName,
        /// The subject whose value is compared.
        subject: NamedOrBlankNode,
        /// The predicate whose value is compared.
        predicate: NamedNode,
        /// The value the subject must hold for the set to happen.
        expected: Term,
        /// The value written in its place.
        new: Term,
    },
    /// `delta` added to `subject`'s integer counter `predicate`, 0 when it holds none.
    CounterAdd {
        /// The graph the counter is held in.
        graph: GraphName,
        /// The subject holding the counter.
        subject: NamedOrBlankNode,
        /// The predicate of the counter.
        predicate: NamedNode,
        /// The amount added, which may be negative.
        delta: i64,
    },
}
