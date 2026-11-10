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
        /// The graph's term index, `None` for the default graph.
        graph: Option<u32>,
        /// The predicate's term index.
        predicate: u32,
        /// The unique predicate's value, a term index.
        value: u32,
        /// Each property and its value, term indices.
        properties: Vec<(u32, u32)>,
    },
    /// As insert-if-absent, and in the same step the `links` between the node and other nodes.
    UpsertAndLink {
        /// The graph's term index, `None` for the default graph.
        graph: Option<u32>,
        /// The predicate's term index.
        predicate: u32,
        /// The unique predicate's value, a term index.
        value: u32,
        /// Each property and its value, term indices.
        properties: Vec<(u32, u32)>,
        /// The links written with the node.
        links: Vec<Link>,
    },
    /// `subject`'s single value of `predicate` replaced by `new` when it is `expected`; else the transaction is refused
    /// with the value found.
    CompareAndSet {
        /// The graph's term index, `None` for the default graph.
        graph: Option<u32>,
        /// The subject's term index.
        subject: u32,
        /// The predicate's term index.
        predicate: u32,
        /// The value `subject` must hold, a term index.
        expected: u32,
        /// The value replacing it, a term index.
        new: u32,
    },
    /// `delta` added to `subject`'s integer counter `predicate`, reading nothing.
    CounterAdd {
        /// The graph's term index, `None` for the default graph.
        graph: Option<u32>,
        /// The subject's term index.
        subject: u32,
        /// The predicate's term index.
        predicate: u32,
        /// The amount added.
        delta: i64,
    },
    /// The instance `instance` names deleted at commit: an entity with the relation instances its `@on_delete`
    /// actions reach in the partition, and its tombstone; a retraction on an instance of a `@bitemporal` type.
    DeleteInstance {
        /// The graph's term index, `None` for the default graph.
        graph: Option<u32>,
        /// The instance's IRI, a term index: an entity's, a node-encoded relation instance's, or an edge-encoded
        /// instance's reifier's.
        instance: u32,
    },
    /// The instance `instance` names retracted at commit: its `asserted` closed and `@on_retract` applied to the
    /// relation instances naming it in the partition.
    RetractInstance {
        /// The graph's term index, `None` for the default graph.
        graph: Option<u32>,
        /// The instance's IRI, a term index.
        instance: u32,
    },
}
