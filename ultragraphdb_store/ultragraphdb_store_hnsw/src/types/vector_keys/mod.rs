/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{Graph, Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_tok::{VEC_DEAD, VEC_ENTRY, VEC_KEYWORD};

/// The vector-index list kind of the entry point.
pub const ENTRY_LIST: u8 = 0;
/// The vector-index list kind of a node's neighbour lists.
pub const NEIGHBOUR_LIST: u8 = 1;
/// The vector-index list kind of the dead nodes.
pub const DEAD_LIST: u8 = 2;

/// Where the HNSW index of one predicate in one graph of a tenant keeps its lists and reads its vectors, by the quad
/// keys: each node's vector in the predicate's forward list of the node; its neighbour lists, the entry point and the
/// dead nodes in the vector-index keyspace under the graph and predicate, the entry point and the dead list under the
/// predicate's own ID, which their list kinds keep apart from a node's neighbour lists. `name` labels the values an insert writes,
/// as Dgraph's attributes `<name>__vector_`, `<name>__vector_entry` and `<name>__vector_dead` named its lists.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VectorKeys {
    name: String,
    tenant: u64,
    graph: Graph,
    predicate: SystemId,
}

impl VectorKeys {
    /// The keys of the vector index `name` of `predicate` in `graph` of `tenant`.
    pub fn new(name: impl Into<String>, tenant: u64, graph: Graph, predicate: SystemId) -> Self {
        Self {
            name: name.into(),
            tenant,
            graph,
            predicate,
        }
    }

    /// The index's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The tenant the index belongs to.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The graph the index belongs to.
    #[must_use]
    pub fn graph(&self) -> Graph {
        self.graph
    }

    /// The predicate the index indexes.
    #[must_use]
    pub fn predicate(&self) -> SystemId {
        self.predicate
    }

    /// The forward list of `node`, which holds its vector.
    #[must_use]
    pub fn vector(&self, node: SystemId) -> Vec<u8> {
        QuadKey::Forward {
            tenant: self.tenant,
            graph: self.graph,
            predicate: self.predicate,
            subject: node,
        }
        .to_bytes()
    }

    /// The prefix of every node's forward list.
    pub fn vector_prefix(&self) -> Vec<u8> {
        key_prefix(
            Keyspace::Forward,
            false,
            self.tenant,
            Some(self.graph),
            Some(self.predicate),
        )
    }

    /// The key of `node`'s neighbour lists, every level's in one matrix.
    #[must_use]
    pub fn neighbours(&self, node: SystemId) -> Vec<u8> {
        self.list(NEIGHBOUR_LIST, node)
    }

    /// The key of the entry point, the 16 big-endian bytes of its node's system ID, under the predicate's ID.
    pub fn entry(&self) -> Vec<u8> {
        self.list(ENTRY_LIST, self.predicate)
    }

    /// The key of the dead nodes, a JSON array of their IDs, under the predicate's ID.
    pub fn dead(&self) -> Vec<u8> {
        self.list(DEAD_LIST, self.predicate)
    }

    /// The label of the neighbour lists' values, `<name>__vector_`.
    pub fn neighbours_label(&self) -> String {
        format!("{}{VEC_KEYWORD}", self.name)
    }

    /// The label of the entry point's value, `<name>__vector_entry`.
    pub fn entry_label(&self) -> String {
        format!("{}{VEC_ENTRY}", self.name)
    }

    /// The label of the dead list's value, `<name>__vector_dead`.
    pub fn dead_label(&self) -> String {
        format!("{}{VEC_DEAD}", self.name)
    }

    /// The node `key` names, when it is a node's forward list of the index's predicate.
    #[must_use]
    pub fn node_of(&self, key: &[u8]) -> Option<SystemId> {
        match QuadKey::parse(key) {
            Ok((
                QuadKey::Forward {
                    tenant,
                    graph,
                    predicate,
                    subject,
                },
                None,
            )) if (tenant, graph, predicate) == (self.tenant, self.graph, self.predicate) => {
                Some(subject)
            }
            _ => None,
        }
    }

    fn list(&self, list: u8, node: SystemId) -> Vec<u8> {
        QuadKey::VectorIndex {
            tenant: self.tenant,
            graph: self.graph,
            predicate: self.predicate,
            list,
            node,
        }
        .to_bytes()
    }
}
