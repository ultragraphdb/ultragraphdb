/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The keyspace a quad key belongs to: its first byte, with [`crate::SPLIT_FLAG`] set in the key of one part of a
/// split list. The bytes are stored: none changes meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Keyspace {
    /// A catalog entry: an entry kind, then the predicate's or type's ID.
    Catalog,
    /// A subject's objects of a predicate: graph, predicate, subject.
    Forward,
    /// An object's subjects of a predicate: graph, predicate, object.
    Reverse,
    /// The predicates a subject has: graph, subject.
    SubjectPredicates,
    /// The predicates that point at an object: graph, object.
    ObjectPredicates,
    /// The subjects a predicate's token is indexed under: graph, predicate, tokenizer, token.
    ValueIndex,
    /// The subjects with a number of a predicate's objects: graph, predicate, direction, count.
    CountIndex,
    /// A predicate's HNSW lists: graph, predicate, list kind, node.
    VectorIndex,
    /// A statement's record: graph, statement ID.
    Statements,
    /// A predicate's statistics in a graph: graph, predicate.
    Statistics,
    /// A referring partition's registration mark of another partition's entity, per bucket group: entity, group.
    Marks,
    /// An entity's referrer set, at its home: entity.
    Referrers,
    /// A dictionary entry, in the keyspace without versions: a kind, then a term's 128-bit hash or an ID.
    Dictionary,
}

/// The flag on the keyspace byte of a part of a split list.
pub const SPLIT_FLAG: u8 = 0x80;

impl Keyspace {
    /// The keyspace byte its keys begin with, without the split flag.
    #[must_use]
    pub fn byte(self) -> u8 {
        match self {
            Keyspace::Catalog => 0x01,
            Keyspace::Forward => 0x10,
            Keyspace::Reverse => 0x11,
            Keyspace::SubjectPredicates => 0x12,
            Keyspace::ObjectPredicates => 0x13,
            Keyspace::ValueIndex => 0x14,
            Keyspace::CountIndex => 0x15,
            Keyspace::VectorIndex => 0x16,
            Keyspace::Statements => 0x17,
            Keyspace::Statistics => 0x18,
            Keyspace::Marks => 0x19,
            Keyspace::Referrers => 0x1A,
            Keyspace::Dictionary => 0x1B,
        }
    }

    /// The keyspace of keyspace byte `byte`, the split flag cleared, and whether it was set.
    #[must_use]
    pub fn from_byte(byte: u8) -> Option<(Self, bool)> {
        let keyspace = match byte & !SPLIT_FLAG {
            0x01 => Keyspace::Catalog,
            0x10 => Keyspace::Forward,
            0x11 => Keyspace::Reverse,
            0x12 => Keyspace::SubjectPredicates,
            0x13 => Keyspace::ObjectPredicates,
            0x14 => Keyspace::ValueIndex,
            0x15 => Keyspace::CountIndex,
            0x16 => Keyspace::VectorIndex,
            0x17 => Keyspace::Statements,
            0x18 => Keyspace::Statistics,
            0x19 => Keyspace::Marks,
            0x1A => Keyspace::Referrers,
            0x1B => Keyspace::Dictionary,
            _ => return None,
        };
        debug_assert_eq!(
            keyspace.byte(),
            byte & !SPLIT_FLAG,
            "a keyspace reads back from its byte"
        );
        Some((keyspace, byte & SPLIT_FLAG != 0))
    }

    /// Whether the keyspace holds posting lists, whose large ones split into parts.
    #[must_use]
    pub fn is_list(self) -> bool {
        matches!(
            self,
            Keyspace::Forward
                | Keyspace::Reverse
                | Keyspace::SubjectPredicates
                | Keyspace::ObjectPredicates
                | Keyspace::ValueIndex
                | Keyspace::CountIndex
                | Keyspace::VectorIndex
        )
    }

    /// Whether its lists tell postings apart by their values as well as their IDs: every list keyspace but the vector
    /// index's, whose lists hold one value each, overwritten in place.
    #[must_use]
    pub fn compares_values(self) -> bool {
        self.is_list() && self != Keyspace::VectorIndex
    }

    /// Whether its keys name a graph after the tenant: every keyspace but the catalog's, the marks', the
    /// referrers' and the dictionary's.
    #[must_use]
    pub fn has_graph(self) -> bool {
        !matches!(
            self,
            Keyspace::Catalog | Keyspace::Marks | Keyspace::Referrers | Keyspace::Dictionary
        )
    }

    /// Whether its keys name a predicate after the graph.
    #[must_use]
    pub fn has_predicate(self) -> bool {
        let has = matches!(
            self,
            Keyspace::Forward
                | Keyspace::Reverse
                | Keyspace::ValueIndex
                | Keyspace::CountIndex
                | Keyspace::VectorIndex
                | Keyspace::Statistics
        );
        debug_assert!(!has || self.has_graph(), "a predicate follows a graph");
        has
    }
}
