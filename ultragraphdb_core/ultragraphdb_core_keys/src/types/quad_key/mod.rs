/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod quad_key_parse;
mod quad_key_split;

use ultragraphdb_core_terms::EncodedTerm;

use crate::types::count_direction::CountDirection;
use crate::types::keyspace::Keyspace;
use crate::types::region::Region;

/// The graph a statement belongs to when it names none.
pub const DEFAULT_GRAPH: u64 = 0;

/// A key of the quad layout, by its fields: what a key is built from and what parsing one returns. Every key is the
/// keyspace byte, the namespace as a u64 big-endian, then the fields in the order a variant lists them; UIDs, IDs and
/// node numbers are u64 big-endian, so a predicate's forward keys sort by subject, and an object is its encoding,
/// which ends where its tag says. No key carries a vector.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum QuadKey {
    Catalog {
        namespace: u64,
        kind: u8,
        uid: u64,
    },
    Forward {
        namespace: u64,
        graph: u64,
        predicate: u64,
        subject: u64,
    },
    Reverse {
        namespace: u64,
        graph: u64,
        predicate: u64,
        object: EncodedTerm,
    },
    SubjectPredicates {
        namespace: u64,
        graph: u64,
        subject: u64,
    },
    ObjectPredicates {
        namespace: u64,
        graph: u64,
        object: EncodedTerm,
    },
    /// A token is raw bytes to the key's end, or to its split start.
    ValueIndex {
        namespace: u64,
        graph: u64,
        predicate: u64,
        tokenizer: u8,
        token: Vec<u8>,
    },
    CountIndex {
        namespace: u64,
        graph: u64,
        predicate: u64,
        direction: CountDirection,
        count: u32,
    },
    VectorIndex {
        namespace: u64,
        graph: u64,
        predicate: u64,
        list: u8,
        node: u64,
    },
    Statement {
        namespace: u64,
        graph: u64,
        statement: u64,
    },
    Statistics {
        namespace: u64,
        graph: u64,
        predicate: u64,
    },
}

impl QuadKey {
    pub fn keyspace(&self) -> Keyspace {
        match self {
            QuadKey::Catalog { .. } => Keyspace::Catalog,
            QuadKey::Forward { .. } => Keyspace::Forward,
            QuadKey::Reverse { .. } => Keyspace::Reverse,
            QuadKey::SubjectPredicates { .. } => Keyspace::SubjectPredicates,
            QuadKey::ObjectPredicates { .. } => Keyspace::ObjectPredicates,
            QuadKey::ValueIndex { .. } => Keyspace::ValueIndex,
            QuadKey::CountIndex { .. } => Keyspace::CountIndex,
            QuadKey::VectorIndex { .. } => Keyspace::VectorIndex,
            QuadKey::Statement { .. } => Keyspace::Statements,
            QuadKey::Statistics { .. } => Keyspace::Statistics,
        }
    }

    pub fn namespace(&self) -> u64 {
        match self {
            QuadKey::Catalog { namespace, .. }
            | QuadKey::Forward { namespace, .. }
            | QuadKey::Reverse { namespace, .. }
            | QuadKey::SubjectPredicates { namespace, .. }
            | QuadKey::ObjectPredicates { namespace, .. }
            | QuadKey::ValueIndex { namespace, .. }
            | QuadKey::CountIndex { namespace, .. }
            | QuadKey::VectorIndex { namespace, .. }
            | QuadKey::Statement { namespace, .. }
            | QuadKey::Statistics { namespace, .. } => *namespace,
        }
    }

    /// The graph, for every keyspace but the catalog's.
    pub fn graph(&self) -> Option<u64> {
        match self {
            QuadKey::Catalog { .. } => None,
            QuadKey::Forward { graph, .. }
            | QuadKey::Reverse { graph, .. }
            | QuadKey::SubjectPredicates { graph, .. }
            | QuadKey::ObjectPredicates { graph, .. }
            | QuadKey::ValueIndex { graph, .. }
            | QuadKey::CountIndex { graph, .. }
            | QuadKey::VectorIndex { graph, .. }
            | QuadKey::Statement { graph, .. }
            | QuadKey::Statistics { graph, .. } => Some(*graph),
        }
    }

    /// The predicate, for the keyspaces whose keys name one.
    pub fn predicate(&self) -> Option<u64> {
        match self {
            QuadKey::Forward { predicate, .. }
            | QuadKey::Reverse { predicate, .. }
            | QuadKey::ValueIndex { predicate, .. }
            | QuadKey::CountIndex { predicate, .. }
            | QuadKey::VectorIndex { predicate, .. }
            | QuadKey::Statistics { predicate, .. } => Some(*predicate),
            _ => None,
        }
    }

    /// The region the key belongs to.
    pub fn region(&self) -> Region {
        Region::new(
            self.keyspace(),
            self.namespace(),
            self.graph(),
            self.predicate(),
        )
    }

    /// The key's bytes. An object that is a vector is written as its encoding, which no key should carry; the store
    /// keeps vectors out of every key that names an object.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut key = Vec::with_capacity(48);
        key.push(self.keyspace().byte());
        key.extend_from_slice(&self.namespace().to_be_bytes());
        let u64s = |key: &mut Vec<u8>, values: &[u64]| {
            values
                .iter()
                .for_each(|v| key.extend_from_slice(&v.to_be_bytes()));
        };
        match self {
            QuadKey::Catalog { kind, uid, .. } => {
                key.push(*kind);
                u64s(&mut key, &[*uid]);
            }
            QuadKey::Forward {
                graph,
                predicate,
                subject,
                ..
            } => u64s(&mut key, &[*graph, *predicate, *subject]),
            QuadKey::Reverse {
                graph,
                predicate,
                object,
                ..
            } => {
                u64s(&mut key, &[*graph, *predicate]);
                key.extend_from_slice(object.as_bytes());
            }
            QuadKey::SubjectPredicates { graph, subject, .. } => {
                u64s(&mut key, &[*graph, *subject])
            }
            QuadKey::ObjectPredicates { graph, object, .. } => {
                u64s(&mut key, &[*graph]);
                key.extend_from_slice(object.as_bytes());
            }
            QuadKey::ValueIndex {
                graph,
                predicate,
                tokenizer,
                token,
                ..
            } => {
                u64s(&mut key, &[*graph, *predicate]);
                key.push(*tokenizer);
                key.extend_from_slice(token);
            }
            QuadKey::CountIndex {
                graph,
                predicate,
                direction,
                count,
                ..
            } => {
                u64s(&mut key, &[*graph, *predicate]);
                key.push(direction.byte());
                key.extend_from_slice(&count.to_be_bytes());
            }
            QuadKey::VectorIndex {
                graph,
                predicate,
                list,
                node,
                ..
            } => {
                u64s(&mut key, &[*graph, *predicate]);
                key.push(*list);
                u64s(&mut key, &[*node]);
            }
            QuadKey::Statement {
                graph, statement, ..
            } => u64s(&mut key, &[*graph, *statement]),
            QuadKey::Statistics {
                graph, predicate, ..
            } => u64s(&mut key, &[*graph, *predicate]),
        }
        key
    }
}

/// The prefix every key of `keyspace` in `namespace` begins with, narrowed to a graph and, when the keyspace's keys
/// name one, a predicate: what a scan of a predicate's subjects or a drop of a graph reads. A split part's prefix sets
/// the flag in the first byte.
pub fn key_prefix(
    keyspace: Keyspace,
    split: bool,
    namespace: u64,
    graph: Option<u64>,
    predicate: Option<u64>,
) -> Vec<u8> {
    let mut prefix = vec![keyspace.byte() | if split { crate::SPLIT_FLAG } else { 0 }];
    prefix.extend_from_slice(&namespace.to_be_bytes());
    if let Some(graph) = graph {
        prefix.extend_from_slice(&graph.to_be_bytes());
        if let Some(predicate) = predicate.filter(|_| keyspace.has_predicate()) {
            prefix.extend_from_slice(&predicate.to_be_bytes());
        }
    }
    prefix
}
