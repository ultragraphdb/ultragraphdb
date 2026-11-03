/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::IdError;
use ultragraphdb_core_keys::{
    DictionaryEntry, Keyspace, QuadKey, QuadKeyErrorEnum, Region, SPLIT_FLAG,
};
use ultragraphdb_core_terms::Hash128;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::quad_key::quad_key_tests::id;

/// The ID (type 1, bucket 42, sequence 9), hand-computed: type 00 01, sequence 9 in 8 bytes, bucket 42 in 3, 3 zero.
const ID_42_9: [u8; 16] = [0, 1, 0, 0, 0, 0, 0, 0, 0, 9, 0, 0, 42, 0, 0, 0];

#[test]
fn test_each_entry_has_its_kind_byte_and_16_bytes() -> TestResult {
    let hash = Hash128::from_bytes([0x5A; 16]);
    assert_eq!(DictionaryEntry::Term(hash).kind(), 1);
    assert_eq!(DictionaryEntry::Id(id(1, 42, 9)?).kind(), 2);
    assert_eq!(DictionaryEntry::Literal(hash).kind(), 3);
    assert_eq!(DictionaryEntry::Term(hash).to_bytes(), [0x5A; 16]);
    assert_eq!(DictionaryEntry::Literal(hash).to_bytes(), [0x5A; 16]);
    assert_eq!(DictionaryEntry::Id(id(1, 42, 9)?).to_bytes(), ID_42_9);
    Ok(())
}

#[test]
fn test_a_dictionary_key_is_its_keyspace_namespace_kind_and_16_bytes() -> TestResult {
    // term-dictionary, "The dictionary is a keyspace without versions": keyspace byte 0x1B, namespace 7, kind 2, the ID.
    let key = QuadKey::Dictionary {
        namespace: 7,
        entry: DictionaryEntry::Id(id(1, 42, 9)?),
    };
    let mut want = vec![0x1B, 0, 0, 0, 0, 0, 0, 0, 7, 0x02];
    want.extend_from_slice(&ID_42_9);
    assert_eq!(key.to_bytes(), want);
    assert_eq!(key.keyspace(), Keyspace::Dictionary);
    assert_eq!(
        (key.namespace(), key.graph(), key.predicate()),
        (7, None, None)
    );
    assert_eq!(QuadKey::parse(&want)?, (key, None));
    Ok(())
}

#[test]
fn test_every_entry_round_trips() -> TestResult {
    for entry in [
        DictionaryEntry::Term(Hash128::from_bytes([0; 16])),
        DictionaryEntry::Term(Hash128::from_bytes([0xFF; 16])),
        DictionaryEntry::Id(id(u16::MAX, 0xFF_FFFF, u64::MAX)?),
        DictionaryEntry::Literal(Hash128::of(b"a string over fifteen bytes")),
    ] {
        let key = QuadKey::Dictionary {
            namespace: u64::MAX,
            entry,
        };
        let bytes = key.to_bytes();
        assert_eq!(bytes.len(), 1 + 8 + 1 + 16, "{entry:?}");
        assert_eq!(QuadKey::parse(&bytes)?, (key, None), "{entry:?}");
    }
    Ok(())
}

#[test]
fn test_dictionary_keys_of_a_namespace_share_its_region() -> TestResult {
    // quad-storage, "Every key belongs to one region": a dictionary key's region is its keyspace and namespace.
    let key = |namespace, entry| QuadKey::Dictionary { namespace, entry }.to_bytes();
    let a = Region::of(&key(7, DictionaryEntry::Id(id(1, 42, 9)?)))?;
    let b = Region::of(&key(
        7,
        DictionaryEntry::Literal(Hash128::from_bytes([1; 16])),
    ))?;
    let c = Region::of(&key(8, DictionaryEntry::Id(id(1, 42, 9)?)))?;
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert_eq!(
        (a.keyspace(), a.namespace(), a.graph(), a.predicate()),
        (Keyspace::Dictionary, 7, None, None)
    );
    Ok(())
}

#[test]
fn test_an_unknown_kind_is_refused() -> TestResult {
    for kind in [0x00u8, 0x04, 0xFF] {
        let mut key = vec![0x1B, 0, 0, 0, 0, 0, 0, 0, 7, kind];
        key.extend_from_slice(&ID_42_9);
        assert_eq!(
            QuadKey::parse(&key).map_err(|e| e.0),
            Err(QuadKeyErrorEnum::UnknownKind {
                key: key.clone(),
                kind
            })
        );
    }
    Ok(())
}

#[test]
fn test_an_id_entry_that_is_no_valid_id_is_refused() -> TestResult {
    let mut key = vec![0x1B, 0, 0, 0, 0, 0, 0, 0, 7, 0x02];
    key.extend_from_slice(&ID_42_9);
    // The ID's sequence, bytes 2 to 9 of its 16, all zero.
    key.get_mut(12..20).context("the sequence")?.fill(0);
    assert_eq!(
        QuadKey::parse(&key).map_err(|e| e.0),
        Err(QuadKeyErrorEnum::InvalidId {
            key: key.clone(),
            field: "ID",
            error: IdError::zero_sequence()
        })
    );
    Ok(())
}

#[test]
fn test_a_dictionary_key_cut_short_running_on_or_split_is_refused() -> TestResult {
    let mut whole = vec![0x1B, 0, 0, 0, 0, 0, 0, 0, 7, 0x01];
    whole.extend_from_slice(&[0x5A; 16]);
    for len in 9..whole.len() {
        let cut = whole.get(..len).context("the cut")?;
        assert_eq!(
            QuadKey::parse(cut).map_err(|e| e.0),
            Err(QuadKeyErrorEnum::Truncated { key: cut.to_vec() })
        );
    }
    let mut longer = whole.clone();
    longer.push(0);
    assert_eq!(
        QuadKey::parse(&longer).map_err(|e| e.0),
        Err(QuadKeyErrorEnum::TrailingBytes {
            key: longer.clone()
        })
    );
    let mut flagged = whole.clone();
    if let Some(first) = flagged.first_mut() {
        *first |= SPLIT_FLAG;
    }
    assert_eq!(
        QuadKey::parse(&flagged).map_err(|e| e.0),
        Err(QuadKeyErrorEnum::UnknownKeyspace {
            key: flagged.clone()
        })
    );
    Ok(())
}
