/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::Literal;
use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_keys::{
    CountDirection, DEFAULT_GRAPH, DictionaryEntry, Graph, Keyspace, QuadKey, ROOT_TENANT,
    key_prefix,
};
use ultragraphdb_core_terms::{EncodedTerm, Hash128, TermError};
use ultragraphdb_tool_bdd::{Context, TestResult};

pub fn id(prefix: u16, bucket: u32, sequence: u64) -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::new(prefix)?,
        Bucket::new(bucket)?,
        Sequence::new(sequence)?,
    ))
}

pub fn literal(lexical: &str) -> TestResult<EncodedTerm> {
    Ok(
        EncodedTerm::literal(Literal::new_simple_literal(lexical).as_ref(), |_| {
            Err::<SystemId, _>(TermError::malformed(Vec::new(), "no datatype ID is needed"))
        })?
        .into_term(),
    )
}

/// The named graph every key of [`every_keyspace`] but the forward and statistics keys is in.
pub fn graph() -> TestResult<Graph> {
    Ok(Graph::Named(id(1, 3, 3)?))
}

/// One key of every keyspace in tenant 4, every ID distinct from the others, the reverse key twice: with an ID
/// object and a hashed literal.
pub fn every_keyspace() -> TestResult<Vec<QuadKey>> {
    let mut keys = data_keys()?;
    keys.extend(index_keys()?);
    keys.extend(placement_keys()?);
    Ok(keys)
}

/// Keys of the catalog and of the four keyspaces holding quads.
fn data_keys() -> TestResult<Vec<QuadKey>> {
    Ok(vec![
        QuadKey::Catalog {
            tenant: 4,
            kind: 2,
            id: id(1, 9, 90)?,
        },
        QuadKey::Forward {
            tenant: 4,
            graph: DEFAULT_GRAPH,
            predicate: id(1, 9, 9)?,
            subject: id(1, 0xAB_CDEF, 1 << 40)?,
        },
        QuadKey::Reverse {
            tenant: 4,
            graph: graph()?,
            predicate: id(1, 9, 9)?,
            object: EncodedTerm::id(id(1, 7, 7)?),
        },
        QuadKey::Reverse {
            tenant: 4,
            graph: graph()?,
            predicate: id(1, 9, 9)?,
            object: literal("a string over fifteen bytes")?,
        },
        QuadKey::SubjectPredicates {
            tenant: 4,
            graph: graph()?,
            subject: id(1, 5, 5)?,
        },
        QuadKey::ObjectPredicates {
            tenant: 4,
            graph: graph()?,
            object: literal("short")?,
        },
    ])
}

/// Keys of the index, statement and statistics keyspaces.
fn index_keys() -> TestResult<Vec<QuadKey>> {
    Ok(vec![
        QuadKey::ValueIndex {
            tenant: 4,
            graph: graph()?,
            predicate: id(1, 9, 9)?,
            tokenizer: 2,
            token: b"\x00ab\xff".to_vec(),
        },
        QuadKey::ValueIndex {
            tenant: 4,
            graph: graph()?,
            predicate: id(1, 9, 9)?,
            tokenizer: 2,
            token: Vec::new(),
        },
        QuadKey::CountIndex {
            tenant: 4,
            graph: graph()?,
            predicate: id(1, 9, 9)?,
            direction: CountDirection::Reverse,
            count: 70_000,
        },
        QuadKey::VectorIndex {
            tenant: 4,
            graph: graph()?,
            predicate: id(1, 9, 9)?,
            list: 1,
            node: id(1, 8, 8)?,
        },
        QuadKey::Statement {
            tenant: 4,
            graph: graph()?,
            statement: id(2, 5, 11)?,
        },
        QuadKey::Statistics {
            tenant: 4,
            graph: DEFAULT_GRAPH,
            predicate: id(1, 9, 9)?,
        },
    ])
}

/// Keys of the marks, the referrers and the dictionary.
fn placement_keys() -> TestResult<Vec<QuadKey>> {
    Ok(vec![
        QuadKey::Mark {
            tenant: 4,
            entity: id(1, 42, 9)?,
            group: 0x2_ABCD,
        },
        QuadKey::Referrers {
            tenant: 4,
            entity: id(1, 42, 9)?,
        },
        QuadKey::Dictionary {
            tenant: 4,
            entry: DictionaryEntry::Id(id(1, 42, 10)?),
        },
        QuadKey::Dictionary {
            tenant: 4,
            entry: DictionaryEntry::Term(Hash128::from_bytes([0x5A; 16])),
        },
        QuadKey::Dictionary {
            tenant: 4,
            entry: DictionaryEntry::Literal(Hash128::from_bytes([0xA5; 16])),
        },
    ])
}

#[test]
fn test_every_keyspace_round_trips_and_reports_its_fields() -> TestResult {
    let keys = every_keyspace()?;
    assert_eq!(
        keys.iter()
            .map(QuadKey::keyspace)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        13,
        "every keyspace is covered"
    );
    for key in keys {
        let bytes = key.to_bytes();
        assert_eq!(bytes.first(), Some(&key.keyspace().byte()));
        assert_eq!(
            bytes.get(1..9),
            Some(&4u64.to_be_bytes()[..]),
            "the tenant follows the keyspace byte"
        );
        assert_eq!(QuadKey::parse(&bytes), Ok((key.clone(), None)), "{key:?}");
        assert_eq!(key.tenant(), 4);
        assert_eq!(key.graph().is_some(), key.keyspace().has_graph(), "{key:?}");
        assert_eq!(
            key.predicate().is_some(),
            key.keyspace().has_predicate(),
            "{key:?}"
        );
    }
    Ok(())
}

#[test]
fn test_a_key_reports_its_graph_and_predicate() -> TestResult {
    let keys = every_keyspace()?;
    let forward = keys.get(1).context("the forward key")?;
    assert_eq!(forward.graph(), Some(Graph::Default));
    assert_eq!(forward.predicate(), Some(id(1, 9, 9)?));
    let subject_predicates = keys.get(4).context("the subject-predicates key")?;
    assert_eq!(subject_predicates.graph(), Some(graph()?));
    assert_eq!(subject_predicates.predicate(), None);
    Ok(())
}

/// The scenario's ID (type 1, bucket 7, sequence 7) as its 16 bytes: 00 01 | 00×7 07 | 00 00 07 | 00 00 00.
const PREDICATE_7: [u8; 16] = [0, 1, 0, 0, 0, 0, 0, 0, 0, 7, 0, 0, 7, 0, 0, 0];
/// The ID (type 1, bucket 42, sequence 42): 00 01 | 00×7 2A | 00 00 2A | 00 00 00.
const SUBJECT_42: [u8; 16] = [0, 1, 0, 0, 0, 0, 0, 0, 0, 0x2A, 0, 0, 0x2A, 0, 0, 0];

#[test]
fn test_a_forward_key_is_its_hand_computed_bytes() -> TestResult {
    let key = QuadKey::Forward {
        tenant: 133,
        graph: DEFAULT_GRAPH,
        predicate: id(1, 7, 7)?,
        subject: id(1, 42, 42)?,
    };
    let mut want = vec![0x10, 0, 0, 0, 0, 0, 0, 0, 133];
    want.extend_from_slice(&[0u8; 16]);
    want.extend_from_slice(&PREDICATE_7);
    want.extend_from_slice(&SUBJECT_42);
    assert_eq!(key.to_bytes(), want);
    Ok(())
}

#[test]
fn test_mark_and_referrer_keys_are_their_hand_computed_bytes() -> TestResult {
    let mark = QuadKey::Mark {
        tenant: 7,
        entity: id(1, 42, 42)?,
        group: 0x2_ABCD,
    };
    let mut want = vec![0x19, 0, 0, 0, 0, 0, 0, 0, 7];
    want.extend_from_slice(&SUBJECT_42);
    want.extend_from_slice(&[0x02, 0xAB, 0xCD]);
    assert_eq!(mark.to_bytes(), want);

    let referrers = QuadKey::Referrers {
        tenant: 7,
        entity: id(1, 42, 42)?,
    };
    let mut want_referrers = vec![0x1A, 0, 0, 0, 0, 0, 0, 0, 7];
    want_referrers.extend_from_slice(&SUBJECT_42);
    assert_eq!(referrers.to_bytes(), want_referrers);
    Ok(())
}

#[test]
fn test_a_mark_holds_every_group_of_18_bits() -> TestResult {
    // The smallest and the largest group, 0 and 2^18 - 1 = 0x3_FFFF, as 3 big-endian bytes.
    for (group, bytes) in [(0u32, [0u8, 0, 0]), (0x3_FFFF, [0x03, 0xFF, 0xFF])] {
        let key = QuadKey::Mark {
            tenant: ROOT_TENANT,
            entity: id(1, 1, 1)?,
            group,
        };
        assert!(key.to_bytes().ends_with(&bytes), "{group:#x}");
        assert_eq!(QuadKey::parse(&key.to_bytes()), Ok((key, None)));
    }
    Ok(())
}

#[test]
fn test_ids_at_their_bounds_round_trip() -> TestResult {
    for subject in [
        id(1, 0, 1)?,
        id(u16::MAX, Bucket::MAX.get(), u64::MAX)?,
        id(2, Bucket::MAX.get(), 1)?,
    ] {
        let key = QuadKey::SubjectPredicates {
            tenant: u64::MAX,
            graph: Graph::Named(subject),
            subject,
        };
        assert_eq!(QuadKey::parse(&key.to_bytes()), Ok((key, None)));
    }
    Ok(())
}

#[test]
fn test_a_predicates_forward_keys_sort_as_their_subjects() -> TestResult {
    // Pairs that differ in the type, the sequence or the bucket, each lower one's other fields higher: the key order
    // is the ID order, type then sequence then bucket.
    let ordered = [
        id(1, Bucket::MAX.get(), 1)?,
        id(1, 0, 2)?,
        id(1, 1, 2)?,
        id(2, 0, 1)?,
    ];
    let predicate = id(1, 1, 1)?;
    let keys: Vec<Vec<u8>> = ordered
        .iter()
        .map(|&subject| {
            QuadKey::Forward {
                tenant: 1,
                graph: DEFAULT_GRAPH,
                predicate,
                subject,
            }
            .to_bytes()
        })
        .collect();
    assert!(keys.is_sorted());
    Ok(())
}

#[test]
fn test_a_prefix_covers_exactly_its_keys() -> TestResult {
    for key in every_keyspace()? {
        let bytes = key.to_bytes();
        let ks = key.keyspace();
        let prefixes = [
            key_prefix(ks, false, 4, None, None),
            key_prefix(ks, false, 4, key.graph(), None),
            key_prefix(ks, false, 4, key.graph(), key.predicate()),
        ];
        assert!(prefixes.iter().all(|p| bytes.starts_with(p)), "{key:?}");
        assert!(!bytes.starts_with(&key_prefix(ks, false, 5, None, None)));
        if key.graph().is_some() {
            let other = Graph::Named(id(1, 3, 4)?);
            assert!(!bytes.starts_with(&key_prefix(ks, false, 4, Some(other), None)));
        }
    }
    Ok(())
}

#[test]
fn test_a_prefix_is_the_keyspace_tenant_graph_and_predicate_bytes() -> TestResult {
    let predicate = id(1, 7, 7)?;
    let mut want = vec![0x10 | 0x80, 0, 0, 0, 0, 0, 0, 0, 133];
    want.extend_from_slice(&[0u8; 16]);
    want.extend_from_slice(&PREDICATE_7);
    assert_eq!(
        key_prefix(
            Keyspace::Forward,
            true,
            133,
            Some(DEFAULT_GRAPH),
            Some(predicate)
        ),
        want
    );
    // A keyspace whose keys name no predicate leaves it out; one whose keys name no graph leaves both out.
    assert_eq!(
        key_prefix(
            Keyspace::SubjectPredicates,
            false,
            133,
            Some(DEFAULT_GRAPH),
            Some(predicate)
        )
        .len(),
        1 + 8 + 16
    );
    for keyspace in [
        Keyspace::Catalog,
        Keyspace::Marks,
        Keyspace::Referrers,
        Keyspace::Dictionary,
    ] {
        assert_eq!(
            key_prefix(keyspace, false, 133, Some(DEFAULT_GRAPH), Some(predicate)),
            vec![keyspace.byte(), 0, 0, 0, 0, 0, 0, 0, 133],
            "{keyspace:?}"
        );
    }
    Ok(())
}
