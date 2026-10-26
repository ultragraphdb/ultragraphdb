/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::{Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_tok::{VEC_DEAD, VEC_ENTRY, VEC_KEYWORD};

/// The vector-index list kind of the entry point.
pub const ENTRY_LIST: u8 = 0;
/// The vector-index list kind of a node's neighbour lists.
pub const NEIGHBOUR_LIST: u8 = 1;
/// The vector-index list kind of the dead nodes.
pub const DEAD_LIST: u8 = 2;

/// The node the entry point's and the dead list's keys name, which hold one value each for the whole index.
const INDEX_NODE: u64 = 1;

/// Where the HNSW index of one predicate in one graph of a namespace keeps its lists and reads its vectors, by the quad
/// keys: each node's vector in the predicate's forward list of the node; its neighbour lists, the entry point and the
/// dead nodes in the vector-index keyspace under the graph and predicate. `name` labels the values an insert writes,
/// as Dgraph's attributes `<name>__vector_`, `<name>__vector_entry` and `<name>__vector_dead` named its lists.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VectorKeys {
    name: String,
    namespace: u64,
    graph: u64,
    predicate: u64,
}

impl VectorKeys {
    pub fn new(name: impl Into<String>, namespace: u64, graph: u64, predicate: u64) -> Self {
        Self {
            name: name.into(),
            namespace,
            graph,
            predicate,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    pub fn graph(&self) -> u64 {
        self.graph
    }

    pub fn predicate(&self) -> u64 {
        self.predicate
    }

    /// The forward list of `node`, which holds its vector.
    pub fn vector(&self, node: u64) -> Vec<u8> {
        QuadKey::Forward {
            namespace: self.namespace,
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
            self.namespace,
            Some(self.graph),
            Some(self.predicate),
        )
    }

    /// The key of `node`'s neighbour lists, every level's in one matrix.
    pub fn neighbours(&self, node: u64) -> Vec<u8> {
        self.list(NEIGHBOUR_LIST, node)
    }

    /// The key of the entry point, the 8 big-endian bytes of its node's UID.
    pub fn entry(&self) -> Vec<u8> {
        self.list(ENTRY_LIST, INDEX_NODE)
    }

    /// The key of the dead nodes, a JSON array of UIDs.
    pub fn dead(&self) -> Vec<u8> {
        self.list(DEAD_LIST, INDEX_NODE)
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
    pub fn node_of(&self, key: &[u8]) -> Option<u64> {
        match QuadKey::parse(key) {
            Ok((
                QuadKey::Forward {
                    namespace,
                    graph,
                    predicate,
                    subject,
                },
                None,
            )) if (namespace, graph, predicate) == (self.namespace, self.graph, self.predicate) => {
                Some(subject)
            }
            _ => None,
        }
    }

    fn list(&self, list: u8, node: u64) -> Vec<u8> {
        QuadKey::VectorIndex {
            namespace: self.namespace,
            graph: self.graph,
            predicate: self.predicate,
            list,
            node,
        }
        .to_bytes()
    }
}
