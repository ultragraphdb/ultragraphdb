/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_terms::Hash128;

/// What a dictionary key names after its tenant: a kind byte, then 16 bytes. The kind bytes are stored: none
/// changes meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DictionaryEntry {
    /// Kind 1: an IRI's or a reifier name's ID, keyed by the term's 128-bit hash.
    Term(Hash128),
    /// Kind 2: an ID's term, keyed by the ID.
    Id(SystemId),
    /// Kind 3: the string a literal names by its 128-bit hash.
    Literal(Hash128),
}

impl DictionaryEntry {
    /// The kind byte the entry's key holds before its 16 bytes.
    #[must_use]
    pub fn kind(self) -> u8 {
        match self {
            DictionaryEntry::Term(_) => 0x01,
            DictionaryEntry::Id(_) => 0x02,
            DictionaryEntry::Literal(_) => 0x03,
        }
    }

    /// The entry's 16 bytes: the hash, or the ID's big-endian bytes.
    #[must_use]
    pub fn to_bytes(self) -> [u8; 16] {
        match self {
            DictionaryEntry::Term(hash) | DictionaryEntry::Literal(hash) => *hash.as_bytes(),
            DictionaryEntry::Id(id) => id.to_be_bytes(),
        }
    }
}
