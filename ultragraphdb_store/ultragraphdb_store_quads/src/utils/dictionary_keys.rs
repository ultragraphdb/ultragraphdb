/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{DictionaryEntry, QuadKey};
use ultragraphdb_core_terms::Hash128;

/// The key recording the mapping contract's version, once per store, under no tenant and in no keyspace.
pub const CONTRACT_KEY: &[u8] = b"\xffultragraphdb/contract";

/// The dictionary key of the ID an IRI or a reifier's name has in `tenant`: the dictionary keyspace's byte, the
/// tenant, kind 1 and the term's 128-bit hash. Its value is the ID's 16 bytes.
pub fn term_key(tenant: u64, term: &str) -> Vec<u8> {
    key(tenant, DictionaryEntry::Term(Hash128::of(term.as_bytes())))
}

/// The dictionary key of the term `id` names in `tenant`: kind 2 and the ID's 16 bytes. Its value is the term's
/// text.
pub fn id_key(tenant: u64, id: SystemId) -> Vec<u8> {
    key(tenant, DictionaryEntry::Id(id))
}

/// The dictionary key of the string a literal names by `hash` in `tenant`: kind 3 and the hash. Its value is the
/// string.
pub fn literal_key(tenant: u64, hash: &Hash128) -> Vec<u8> {
    key(tenant, DictionaryEntry::Literal(*hash))
}

/// The dictionary key of `entry` in `tenant`, as the quad layout writes it.
fn key(tenant: u64, entry: DictionaryEntry) -> Vec<u8> {
    QuadKey::Dictionary { tenant, entry }.to_bytes()
}
