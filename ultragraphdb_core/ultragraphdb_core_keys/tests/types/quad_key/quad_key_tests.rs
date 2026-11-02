/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::Literal;
use ultragraphdb_core_keys::{CountDirection, DEFAULT_GRAPH, Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_terms::{EncodedTerm, TermError};
use ultragraphdb_tool_bdd::TestResult;

pub fn literal(lexical: &str) -> TestResult<EncodedTerm> {
    Ok(
        EncodedTerm::literal(Literal::new_simple_literal(lexical).as_ref(), |_| {
            Ok::<_, TermError>(1)
        })?
        .into_term(),
    )
}

/// One key of every keyspace, its objects a UID and a hashed literal.
pub fn every_keyspace() -> TestResult<Vec<QuadKey>> {
    let mut keys = data_keys()?;
    keys.extend(index_keys());
    Ok(keys)
}

/// Keys of the catalog and of the four keyspaces holding quads.
fn data_keys() -> TestResult<Vec<QuadKey>> {
    Ok(vec![
        QuadKey::Catalog {
            namespace: 4,
            kind: 2,
            uid: 9,
        },
        QuadKey::Forward {
            namespace: 4,
            graph: 0,
            predicate: 9,
            subject: 1 << 40,
        },
        QuadKey::Reverse {
            namespace: 4,
            graph: 3,
            predicate: 9,
            object: EncodedTerm::uid(7),
        },
        QuadKey::Reverse {
            namespace: 4,
            graph: 3,
            predicate: 9,
            object: literal("a string over fifteen bytes")?,
        },
        QuadKey::SubjectPredicates {
            namespace: 4,
            graph: 3,
            subject: 5,
        },
        QuadKey::ObjectPredicates {
            namespace: 4,
            graph: 3,
            object: literal("short")?,
        },
    ])
}

/// Keys of the index, statement and statistics keyspaces.
fn index_keys() -> Vec<QuadKey> {
    vec![
        QuadKey::ValueIndex {
            namespace: 4,
            graph: 3,
            predicate: 9,
            tokenizer: 2,
            token: b"\x00ab\xff".to_vec(),
        },
        QuadKey::ValueIndex {
            namespace: 4,
            graph: 3,
            predicate: 9,
            tokenizer: 2,
            token: Vec::new(),
        },
        QuadKey::CountIndex {
            namespace: 4,
            graph: 3,
            predicate: 9,
            direction: CountDirection::Reverse,
            count: 70_000,
        },
        QuadKey::VectorIndex {
            namespace: 4,
            graph: 3,
            predicate: 9,
            list: 1,
            node: 8,
        },
        QuadKey::Statement {
            namespace: 4,
            graph: 3,
            statement: 11,
        },
        QuadKey::Statistics {
            namespace: 4,
            graph: 0,
            predicate: 9,
        },
    ]
}

#[test]
fn test_every_keyspace_round_trips_and_reports_its_fields() -> TestResult {
    for key in every_keyspace()? {
        let bytes = key.to_bytes();
        assert_eq!(bytes[0], key.keyspace().byte());
        assert_eq!(
            &bytes[1..9],
            &4u64.to_be_bytes(),
            "the namespace follows the keyspace byte"
        );
        assert_eq!(QuadKey::parse(&bytes), Ok((key.clone(), None)), "{key:?}");
        assert_eq!(key.namespace(), 4);
        assert_eq!(key.graph().is_none(), key.keyspace() == Keyspace::Catalog);
        assert_eq!(key.predicate().is_some(), key.keyspace().has_predicate());
    }
    Ok(())
}

#[test]
fn test_the_default_graph_is_zero_and_accepted() -> TestResult {
    let key = QuadKey::Forward {
        namespace: 0,
        graph: DEFAULT_GRAPH,
        predicate: 1,
        subject: 1,
    };
    assert_eq!(QuadKey::parse(&key.to_bytes())?.0.graph(), Some(0));
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
        if let Some(graph) = key.graph() {
            assert!(!bytes.starts_with(&key_prefix(ks, false, 4, Some(graph + 1), None)));
        }
        if ks.is_list() {
            let split = key.split_bytes(3)?;
            assert!(split.starts_with(&key_prefix(ks, true, 4, key.graph(), key.predicate())));
            assert!(!split.starts_with(&key_prefix(ks, false, 4, None, None)));
        }
    }
    Ok(())
}
