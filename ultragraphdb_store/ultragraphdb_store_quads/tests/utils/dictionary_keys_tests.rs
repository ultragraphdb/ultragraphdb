/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_keys::{DictionaryEntry, QuadKey, Region};
use ultragraphdb_core_terms::Hash128;
use ultragraphdb_store_quads::{CONTRACT_KEY, id_key, literal_key, term_key};
use ultragraphdb_tool_bdd::TestResult;

/// The dictionary keyspace's byte, 0x1B, and namespace 7 as 8 big-endian bytes, from the layout.
const PREFIX: [u8; 9] = [0x1B, 0, 0, 0, 0, 0, 0, 0, 7];

#[test]
fn test_a_terms_key_is_kind_1_and_its_sha_256_truncated() {
    // SHA-256 of "http://ex.org/a", its first 16 bytes, by Python's hashlib.
    let mut expected = PREFIX.to_vec();
    expected.push(0x01);
    expected.extend_from_slice(&[
        0xa8, 0xb8, 0x65, 0x47, 0x55, 0x53, 0xf3, 0x08, 0x76, 0x33, 0xd7, 0x54, 0x3a, 0x62, 0x27,
        0xa0,
    ]);
    assert_eq!(term_key(7, "http://ex.org/a"), expected);
}

#[test]
fn test_an_ids_key_is_kind_2_and_its_16_bytes() -> TestResult {
    let id = SystemId::new(
        TypePrefix::STATEMENT,
        Bucket::new(0xAB_CDEF)?,
        Sequence::new(42)?,
    );
    let mut expected = PREFIX.to_vec();
    expected.push(0x02);
    expected.extend_from_slice(&[0, 2, 0, 0, 0, 0, 0, 0, 0, 0x2A, 0xAB, 0xCD, 0xEF, 0, 0, 0]);
    assert_eq!(id_key(7, id), expected);
    Ok(())
}

#[test]
fn test_a_literals_key_is_kind_3_and_its_hash() {
    // SHA-256 of "a long string past fifteen bytes", its first 16 bytes, by Python's hashlib.
    let hash = Hash128::from_bytes([
        0x11, 0x0b, 0x11, 0xd3, 0xc0, 0x83, 0xc4, 0xfa, 0x7e, 0x25, 0x62, 0x9d, 0x5f, 0xa7, 0x7f,
        0x11,
    ]);
    assert_eq!(Hash128::of(b"a long string past fifteen bytes"), hash);
    let mut expected = PREFIX.to_vec();
    expected.push(0x03);
    expected.extend_from_slice(hash.as_bytes());
    assert_eq!(literal_key(7, &hash), expected);
}

#[test]
fn test_dictionary_keys_parse_as_dictionary_keys_of_their_namespaces_region() -> TestResult {
    let id = SystemId::new(TypePrefix::RESOURCE, Bucket::new(1)?, Sequence::new(1)?);
    let keys = [
        term_key(7, "x"),
        id_key(7, id),
        literal_key(7, &Hash128::of(b"y")),
    ];
    let entries = [
        DictionaryEntry::Term(Hash128::of(b"x")),
        DictionaryEntry::Id(id),
        DictionaryEntry::Literal(Hash128::of(b"y")),
    ];
    for (key, entry) in keys.iter().zip(entries) {
        let (parsed, split) = QuadKey::parse(key)?;
        assert_eq!(
            parsed,
            QuadKey::Dictionary {
                namespace: 7,
                entry
            }
        );
        assert_eq!(split, None);
        assert_eq!(
            Region::of(key)?,
            Region::of(&keys[0])?,
            "one region per namespace"
        );
    }
    assert_ne!(Region::of(&term_key(8, "x"))?, Region::of(&keys[0])?);
    Ok(())
}

#[test]
fn test_the_contract_key_lies_in_no_namespace() {
    assert_eq!(CONTRACT_KEY.first(), Some(&0xFF));
    assert!(QuadKey::parse(CONTRACT_KEY).is_err());
}
