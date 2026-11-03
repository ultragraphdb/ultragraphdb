/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{Graph, Keyspace, SPLIT_FLAG, key_prefix};
use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{Scan, StoreSnapshot};

/// The keys of one predicate in every graph of a keyspace: its keyspace, split parts or not, namespace, predicate,
/// and the bytes that follow the predicate in each key it covers, a tokenizer's identifier or a count direction. A
/// predicate's keys are one range per graph, since the graph precedes the predicate, so [`PredicatePrefix::graphs`]
/// finds the graphs holding them by a skip scan: a seek to the predicate under one graph, a seek past the graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct PredicatePrefix {
    keyspace: Keyspace,
    split: bool,
    namespace: u64,
    predicate: SystemId,
    rest: Vec<u8>,
}

impl PredicatePrefix {
    pub fn new(
        keyspace: Keyspace,
        split: bool,
        namespace: u64,
        predicate: SystemId,
        rest: Vec<u8>,
    ) -> Self {
        Self {
            keyspace,
            split,
            namespace,
            predicate,
            rest,
        }
    }

    /// The prefix's keys and their split parts, for a keyspace whose lists split.
    pub fn with_parts(
        keyspace: Keyspace,
        namespace: u64,
        predicate: SystemId,
        rest: Vec<u8>,
    ) -> Vec<Self> {
        let mut prefixes = vec![Self::new(
            keyspace,
            false,
            namespace,
            predicate,
            rest.clone(),
        )];
        if keyspace.is_list() {
            prefixes.push(Self::new(keyspace, true, namespace, predicate, rest));
        }
        prefixes
    }

    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    pub fn predicate(&self) -> SystemId {
        self.predicate
    }

    /// The prefix of the keys in `graph`.
    pub fn in_graph(&self, graph: Graph) -> Vec<u8> {
        let mut prefix = key_prefix(
            self.keyspace,
            self.split,
            self.namespace,
            Some(graph),
            Some(self.predicate),
        );
        prefix.extend_from_slice(&self.rest);
        prefix
    }

    /// The graphs some version of a key under the prefix is stored in, in order.
    pub fn graphs<S: StoreSnapshot>(&self, snapshot: &S) -> Result<Vec<Graph>, StoreError> {
        Ok(self.graphs_read(snapshot)?.0)
    }

    /// [`PredicatePrefix::graphs`], with the key ranges the skip scan read, each from its start, inclusive, to its end,
    /// exclusive, or to the last key: the keys from each seek point to the first key found, none of which is held, the
    /// prefix's keys in each graph visited, and from the last seek point to the end of the namespace's keyspace. A key
    /// written under the prefix in any graph lies in one of them. The key found ends its range: a write to it changes
    /// which graph the scan visits only when it is under the prefix, and then it lies in that graph's prefix.
    pub fn graphs_read<S: StoreSnapshot>(
        &self,
        snapshot: &S,
    ) -> Result<(Vec<Graph>, Vec<Range>), StoreError> {
        let namespace = key_prefix(self.keyspace, self.split, self.namespace, None, None);
        let namespace_end = Scan::prefix(namespace.clone()).end().map(<[u8]>::to_vec);
        let mut graphs = Vec::new();
        let mut ranges = Vec::new();
        let mut cursor = namespace.clone();
        // Ends: each turn moves the cursor past the graph it found, and the namespace holds finitely many keys.
        while let Some(item) = snapshot
            .scan(
                &Scan::prefix(namespace.clone())
                    .seek(cursor.clone())
                    .all_versions(),
            )
            .next()
            .transpose()?
        {
            ranges.push((cursor, Some(item.key().to_vec())));
            let Some(field) = item
                .key()
                .get(GRAPH_AT..GRAPH_AT + SystemId::BYTES)
                .map(be_u128)
            else {
                return Ok((graphs, ranges));
            };
            let Some(next) = field.checked_add(1) else {
                return Ok((graphs, ranges));
            };
            let next_cursor = [namespace.as_slice(), &next.to_be_bytes()].concat();
            // A graph field that is no graph holds no key under the prefix: the scan moves past it.
            let Some(graph) = graph_of(field) else {
                cursor = next_cursor;
                continue;
            };
            let prefix = self.in_graph(graph);
            let graph_scan = Scan::prefix(prefix.clone()).all_versions();
            ranges.push((prefix.clone(), graph_scan.end().map(<[u8]>::to_vec)));
            if snapshot.scan(&graph_scan).next().transpose()?.is_some() {
                graphs.push(graph);
            }
            cursor = next_cursor;
        }
        ranges.push((cursor, namespace_end));
        Ok((graphs, ranges))
    }

    /// The prefix in every graph holding a key under it.
    pub fn expand<S: StoreSnapshot>(&self, snapshot: &S) -> Result<Vec<Vec<u8>>, StoreError> {
        Ok(self
            .graphs(snapshot)?
            .into_iter()
            .map(|g| self.in_graph(g))
            .collect())
    }

    /// The prefix as a rebuild record keeps it: its keyspace byte, flag included, its namespace and predicate, and the
    /// rest.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![self.keyspace.byte() | if self.split { SPLIT_FLAG } else { 0 }];
        bytes.extend_from_slice(&self.namespace.to_be_bytes());
        bytes.extend_from_slice(&self.predicate.to_be_bytes());
        bytes.extend_from_slice(&self.rest);
        bytes
    }

    /// The prefix [`PredicatePrefix::to_bytes`] wrote, `None` for bytes it does not write.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let (keyspace, split) = Keyspace::from_byte(*bytes.first()?)?;
        let (namespace, after) = bytes.get(1..)?.split_first_chunk::<8>()?;
        let (predicate, rest) = after.split_at_checked(SystemId::BYTES)?;
        Some(Self {
            keyspace,
            split,
            namespace: u64::from_be_bytes(*namespace),
            predicate: SystemId::decode(predicate).ok()?,
            rest: rest.to_vec(),
        })
    }
}

/// A key range read: its start, inclusive, and its end, exclusive, or `None` to the last key.
pub(crate) type Range = (Vec<u8>, Option<Vec<u8>>);

/// Where a key's graph begins: after its keyspace byte and its namespace.
const GRAPH_AT: usize = 9;

/// The 16 bytes `bytes` holds as a big-endian number.
fn be_u128(bytes: &[u8]) -> u128 {
    let mut b = [0u8; 16];
    b.copy_from_slice(bytes);
    u128::from_be_bytes(b)
}

/// The graph a key's 16 graph bytes, as the number `field`, name: 0 the default graph, a valid system ID a named one,
/// anything else none.
fn graph_of(field: u128) -> Option<Graph> {
    if field == 0 {
        return Some(Graph::Default);
    }
    SystemId::from_u128(field).ok().map(Graph::Named)
}
