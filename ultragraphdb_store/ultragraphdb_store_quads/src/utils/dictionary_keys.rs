/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::Hash128;

/// The tags after the namespace of a key in the keyspace without versions. They are stored: none changes meaning.
pub const IRI_TO_UID: u8 = 0x01;
pub const UID_TO_IRI: u8 = 0x02;
pub const HASH_TO_STRING: u8 = 0x03;
/// The high-water marks of the namespace's leased UIDs and statement IDs.
pub const UID_LEASE: u8 = 0x10;
pub const STATEMENT_LEASE: u8 = 0x11;

/// The key recording the mapping contract's version, once per store, under no namespace's prefix.
pub const CONTRACT_KEY: &[u8] = b"\xffultragraphdb/contract";

fn key(namespace: u64, tag: u8, rest: &[u8]) -> Vec<u8> {
    let mut key = Vec::with_capacity(9 + rest.len());
    key.extend_from_slice(&namespace.to_be_bytes());
    key.push(tag);
    key.extend_from_slice(rest);
    key
}

/// The key of an IRI's UID: the IRI's 128-bit hash, for a fixed width.
pub fn iri_key(namespace: u64, iri: &str) -> Vec<u8> {
    key(
        namespace,
        IRI_TO_UID,
        Hash128::of(iri.as_bytes()).as_bytes(),
    )
}

/// The key of a UID's IRI.
pub fn uid_key(namespace: u64, uid: u64) -> Vec<u8> {
    key(namespace, UID_TO_IRI, &uid.to_be_bytes())
}

/// The key of the string a hash names.
pub fn string_key(namespace: u64, hash: &Hash128) -> Vec<u8> {
    key(namespace, HASH_TO_STRING, hash.as_bytes())
}

/// The key of a lease's high-water mark, `tag` naming the sequence.
pub fn lease_key(namespace: u64, tag: u8) -> Vec<u8> {
    key(namespace, tag, &[])
}

/// The value of a UID's entry: a flag marking a skolemized blank node, then the IRI.
pub fn uid_value(iri: &str, skolem: bool) -> Vec<u8> {
    let mut value = Vec::with_capacity(1 + iri.len());
    value.push(u8::from(skolem));
    value.extend_from_slice(iri.as_bytes());
    value
}
