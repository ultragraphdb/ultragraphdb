/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The keyspace a quad key belongs to: its first byte, with [`crate::SPLIT_FLAG`] set in the key of one part of a
/// split list. The bytes are stored: none changes meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Keyspace {
    /// A catalog entry: an entry kind, then the predicate's or type's UID.
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
}

/// The flag on the keyspace byte of a part of a split list.
pub const SPLIT_FLAG: u8 = 0x80;

impl Keyspace {
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
        }
    }

    /// The keyspace of keyspace byte `byte`, the split flag cleared, and whether it was set.
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
            _ => return None,
        };
        Some((keyspace, byte & SPLIT_FLAG != 0))
    }

    /// Whether the keyspace holds posting lists, whose large ones split into parts.
    pub fn is_list(self) -> bool {
        matches!(
            self,
            Keyspace::Forward
                | Keyspace::Reverse
                | Keyspace::SubjectPredicates
                | Keyspace::ObjectPredicates
                | Keyspace::ValueIndex
                | Keyspace::CountIndex
        )
    }

    /// Whether its keys name a predicate after the graph.
    pub fn has_predicate(self) -> bool {
        matches!(
            self,
            Keyspace::Forward
                | Keyspace::Reverse
                | Keyspace::ValueIndex
                | Keyspace::CountIndex
                | Keyspace::VectorIndex
                | Keyspace::Statistics
        )
    }
}
