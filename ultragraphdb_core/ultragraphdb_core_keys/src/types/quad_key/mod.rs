/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod quad_key_parse;
mod quad_key_split;

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_terms::EncodedTerm;

use crate::types::count_direction::CountDirection;
use crate::types::dictionary_entry::DictionaryEntry;
use crate::types::graph::Graph;
use crate::types::keyspace::Keyspace;
use crate::types::region::Region;

/// The tenant a deployment starts with.
pub const ROOT_TENANT: u64 = 0;

/// The graph a statement belongs to when it names none.
pub const DEFAULT_GRAPH: Graph = Graph::Default;

/// A key of the quad layout, by its fields: what a key is built from and what parsing one returns. Every key is the
/// keyspace byte, the tenant as a u64 big-endian, then the fields in the order a variant lists them. Every ID is a
/// 128-bit system ID written as its 16 big-endian bytes, so a predicate's forward keys sort by subject; the default
/// graph is 16 zero bytes; an object is its encoding, which ends where its tag says; a mark's bucket group is 3
/// big-endian bytes holding its 18 bits. No key carries a vector.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum QuadKey {
    /// A catalog entry's key.
    Catalog {
        /// The tenant.
        tenant: u64,
        /// The entry kind.
        kind: u8,
        /// The predicate's or type's ID.
        id: SystemId,
    },
    /// A key of a subject's objects of a predicate.
    Forward {
        /// The tenant.
        tenant: u64,
        /// The graph.
        graph: Graph,
        /// The predicate.
        predicate: SystemId,
        /// The subject.
        subject: SystemId,
    },
    /// A key of an object's subjects of a predicate; it ends with the object.
    Reverse {
        /// The tenant.
        tenant: u64,
        /// The graph.
        graph: Graph,
        /// The predicate.
        predicate: SystemId,
        /// The object's encoding.
        object: EncodedTerm,
    },
    /// A key of the predicates a subject has.
    SubjectPredicates {
        /// The tenant.
        tenant: u64,
        /// The graph.
        graph: Graph,
        /// The subject.
        subject: SystemId,
    },
    /// A key of the predicates that point at an object.
    ObjectPredicates {
        /// The tenant.
        tenant: u64,
        /// The graph.
        graph: Graph,
        /// The object's encoding.
        object: EncodedTerm,
    },
    /// A token is raw bytes to the key's end, or to its split start.
    ValueIndex {
        /// The tenant.
        tenant: u64,
        /// The graph.
        graph: Graph,
        /// The predicate.
        predicate: SystemId,
        /// The tokenizer.
        tokenizer: u8,
        /// The token.
        token: Vec<u8>,
    },
    /// A key of the subjects with a number of a predicate's objects.
    CountIndex {
        /// The tenant.
        tenant: u64,
        /// The graph.
        graph: Graph,
        /// The predicate.
        predicate: SystemId,
        /// Which end is counted.
        direction: CountDirection,
        /// The number of objects or subjects.
        count: u32,
    },
    /// A key of a predicate's HNSW list.
    VectorIndex {
        /// The tenant.
        tenant: u64,
        /// The graph.
        graph: Graph,
        /// The predicate.
        predicate: SystemId,
        /// The list kind.
        list: u8,
        /// The node.
        node: SystemId,
    },
    /// A statement's record's key.
    Statement {
        /// The tenant.
        tenant: u64,
        /// The graph.
        graph: Graph,
        /// The statement ID.
        statement: SystemId,
    },
    /// A key of a predicate's statistics in a graph.
    Statistics {
        /// The tenant.
        tenant: u64,
        /// The graph.
        graph: Graph,
        /// The predicate.
        predicate: SystemId,
    },
    /// The key of a referring partition's registration mark of another partition's entity, for one bucket group of
    /// the referring subjects.
    Mark {
        /// The tenant.
        tenant: u64,
        /// The entity referred to.
        entity: SystemId,
        /// The bucket group of the referring subjects, the top 18 bits of their bucket, below 2^18.
        group: u32,
    },
    /// The key of an entity's referrer set, at its home.
    Referrers {
        /// The tenant.
        tenant: u64,
        /// The entity.
        entity: SystemId,
    },
    /// The key of a dictionary entry, in the keyspace without versions.
    Dictionary {
        /// The tenant.
        tenant: u64,
        /// The entry's kind and its 16 bytes.
        entry: DictionaryEntry,
    },
}

/// The length of a key's tenant.
const TENANT_LEN: usize = 8;
/// The largest bucket group a mark key holds, 2^18 - 1.
pub(crate) const GROUP_MAX: u32 = (1 << 18) - 1;

impl QuadKey {
    /// The keyspace the key belongs to.
    #[must_use]
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
            QuadKey::Mark { .. } => Keyspace::Marks,
            QuadKey::Referrers { .. } => Keyspace::Referrers,
            QuadKey::Dictionary { .. } => Keyspace::Dictionary,
        }
    }

    /// The tenant.
    #[must_use]
    pub fn tenant(&self) -> u64 {
        match self {
            QuadKey::Catalog { tenant, .. }
            | QuadKey::Forward { tenant, .. }
            | QuadKey::Reverse { tenant, .. }
            | QuadKey::SubjectPredicates { tenant, .. }
            | QuadKey::ObjectPredicates { tenant, .. }
            | QuadKey::ValueIndex { tenant, .. }
            | QuadKey::CountIndex { tenant, .. }
            | QuadKey::VectorIndex { tenant, .. }
            | QuadKey::Statement { tenant, .. }
            | QuadKey::Statistics { tenant, .. }
            | QuadKey::Mark { tenant, .. }
            | QuadKey::Referrers { tenant, .. }
            | QuadKey::Dictionary { tenant, .. } => *tenant,
        }
    }

    /// The graph, for the keyspaces whose keys name one: every keyspace but the catalog's, the marks' and the
    /// referrers'.
    #[must_use]
    pub fn graph(&self) -> Option<Graph> {
        match self {
            QuadKey::Catalog { .. }
            | QuadKey::Mark { .. }
            | QuadKey::Referrers { .. }
            | QuadKey::Dictionary { .. } => None,
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
    #[must_use]
    pub fn predicate(&self) -> Option<SystemId> {
        match self {
            QuadKey::Forward { predicate, .. }
            | QuadKey::Reverse { predicate, .. }
            | QuadKey::ValueIndex { predicate, .. }
            | QuadKey::CountIndex { predicate, .. }
            | QuadKey::VectorIndex { predicate, .. }
            | QuadKey::Statistics { predicate, .. } => Some(*predicate),
            QuadKey::Catalog { .. }
            | QuadKey::SubjectPredicates { .. }
            | QuadKey::ObjectPredicates { .. }
            | QuadKey::Statement { .. }
            | QuadKey::Mark { .. }
            | QuadKey::Referrers { .. }
            | QuadKey::Dictionary { .. } => None,
        }
    }

    /// The region the key belongs to.
    #[must_use]
    pub fn region(&self) -> Region {
        Region::new(
            self.keyspace(),
            self.tenant(),
            self.graph(),
            self.predicate(),
        )
    }

    /// The key's bytes. An object that is a vector is written as its encoding, which no key should carry; the store
    /// keeps vectors out of every key that names an object. A mark's group is written as the low 24 bits of its
    /// value, which is below 2^18 for every group.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut key = Vec::with_capacity(1 + TENANT_LEN + 3 * SystemId::BYTES);
        key.push(self.keyspace().byte());
        key.extend_from_slice(&self.tenant().to_be_bytes());
        // Every keyspace naming a graph and a predicate names them first, in that order.
        if let Some(graph) = self.graph() {
            key.extend_from_slice(&graph.to_be_bytes());
        }
        if let Some(predicate) = self.predicate() {
            key.extend_from_slice(&predicate.to_be_bytes());
        }
        self.push_tail(&mut key);
        debug_assert_eq!(
            key.first(),
            Some(&self.keyspace().byte()),
            "the keyspace byte leads"
        );
        key
    }

    /// Appends the key's fields after its graph and predicate.
    fn push_tail(&self, key: &mut Vec<u8>) {
        match self {
            QuadKey::Catalog { kind, id, .. } => {
                key.push(*kind);
                key.extend_from_slice(&id.to_be_bytes());
            }
            QuadKey::Forward { subject: id, .. }
            | QuadKey::SubjectPredicates { subject: id, .. }
            | QuadKey::Statement { statement: id, .. }
            | QuadKey::Referrers { entity: id, .. } => key.extend_from_slice(&id.to_be_bytes()),
            QuadKey::Reverse { object, .. } | QuadKey::ObjectPredicates { object, .. } => {
                key.extend_from_slice(object.as_bytes());
            }
            QuadKey::ValueIndex {
                tokenizer, token, ..
            } => {
                key.push(*tokenizer);
                key.extend_from_slice(token);
            }
            QuadKey::CountIndex {
                direction, count, ..
            } => {
                key.push(direction.byte());
                key.extend_from_slice(&count.to_be_bytes());
            }
            QuadKey::VectorIndex { list, node, .. } => {
                key.push(*list);
                key.extend_from_slice(&node.to_be_bytes());
            }
            QuadKey::Statistics { .. } => {}
            QuadKey::Mark { entity, group, .. } => {
                debug_assert!(*group <= GROUP_MAX, "a bucket group holds 18 bits");
                key.extend_from_slice(&entity.to_be_bytes());
                let [_, high, middle, low] = group.to_be_bytes();
                key.extend_from_slice(&[high, middle, low]);
            }
            QuadKey::Dictionary { entry, .. } => {
                key.push(entry.kind());
                key.extend_from_slice(&entry.to_bytes());
            }
        }
    }
}

/// The prefix every key of `keyspace` in `tenant` begins with, narrowed to a graph when the keyspace's keys name
/// one and, when they name one too, a predicate: what a scan of a predicate's subjects or a drop of a graph reads. A
/// split part's prefix sets the flag in the first byte. A graph or predicate the keyspace's keys do not name is left
/// out.
#[must_use]
pub fn key_prefix(
    keyspace: Keyspace,
    split: bool,
    tenant: u64,
    graph: Option<Graph>,
    predicate: Option<SystemId>,
) -> Vec<u8> {
    let mut prefix = Vec::with_capacity(1 + TENANT_LEN + 2 * SystemId::BYTES);
    prefix.push(keyspace.byte() | if split { crate::SPLIT_FLAG } else { 0 });
    prefix.extend_from_slice(&tenant.to_be_bytes());
    if let Some(graph) = graph.filter(|_| keyspace.has_graph()) {
        prefix.extend_from_slice(&graph.to_be_bytes());
        if let Some(predicate) = predicate.filter(|_| keyspace.has_predicate()) {
            prefix.extend_from_slice(&predicate.to_be_bytes());
        }
    }
    debug_assert!(
        prefix.len() <= 1 + TENANT_LEN + 2 * SystemId::BYTES,
        "a prefix is at most its three fields"
    );
    prefix
}
