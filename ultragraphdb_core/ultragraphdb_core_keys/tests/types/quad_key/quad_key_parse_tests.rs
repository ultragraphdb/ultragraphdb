/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{Literal, NamedNode};
use ultragraphdb_core_keys::{QuadKey, QuadKeyErrorEnum};
use ultragraphdb_core_terms::{EncodedTerm, FLOAT32_VECTOR, TermError};

use crate::types::quad_key::quad_key_tests::every_keyspace;

#[test]
fn test_every_proper_prefix_of_every_key_is_refused_unless_a_shorter_token_makes_it_a_key() {
    for key in every_keyspace() {
        let bytes = key.to_bytes();
        let splits: Vec<Vec<u8>> = key.split_bytes(42).into_iter().collect();
        for whole in std::iter::once(bytes).chain(splits) {
            for end in 0..whole.len() {
                let prefix = &whole[..end];
                match std::panic::catch_unwind(|| QuadKey::parse(prefix)).expect("no panic") {
                    Err(_) => {}
                    Ok((QuadKey::ValueIndex { .. }, _)) if end >= 26 => {}
                    Ok(parsed) => panic!("{prefix:?} of {key:?} parsed as {parsed:?}"),
                }
            }
        }
    }
}

#[test]
fn test_a_zero_uid_is_refused_in_every_field_but_the_graph() {
    let zero = |key: QuadKey, field: &str| {
        let error = QuadKey::parse(&key.to_bytes()).unwrap_err();
        assert!(
            matches!(&error.0, QuadKeyErrorEnum::ZeroUid { field: f, .. } if *f == field),
            "{key:?}: {error}"
        );
    };
    zero(
        QuadKey::Forward {
            namespace: 1,
            graph: 0,
            predicate: 0,
            subject: 1,
        },
        "predicate",
    );
    zero(
        QuadKey::Forward {
            namespace: 1,
            graph: 0,
            predicate: 1,
            subject: 0,
        },
        "subject",
    );
    zero(
        QuadKey::Reverse {
            namespace: 1,
            graph: 0,
            predicate: 1,
            object: EncodedTerm::uid(0),
        },
        "object",
    );
    zero(
        QuadKey::SubjectPredicates {
            namespace: 1,
            graph: 0,
            subject: 0,
        },
        "subject",
    );
    zero(
        QuadKey::VectorIndex {
            namespace: 1,
            graph: 0,
            predicate: 1,
            list: 0,
            node: 0,
        },
        "node",
    );
    zero(
        QuadKey::Statement {
            namespace: 1,
            graph: 0,
            statement: 0,
        },
        "statement ID",
    );
    zero(
        QuadKey::Catalog {
            namespace: 1,
            kind: 0,
            uid: 0,
        },
        "uid",
    );
}

#[test]
fn test_unreadable_objects_unknown_keyspaces_and_directions_are_refused() {
    let vector = EncodedTerm::literal(
        Literal::new_typed_literal("[1]", NamedNode::new_unchecked(FLOAT32_VECTOR)).as_ref(),
        |_| Ok::<_, TermError>(1),
    )
    .unwrap()
    .into_term();
    let key = QuadKey::Reverse {
        namespace: 1,
        graph: 0,
        predicate: 1,
        object: vector,
    };
    assert!(matches!(
        QuadKey::parse(&key.to_bytes()).unwrap_err().0,
        QuadKeyErrorEnum::VectorObject { .. }
    ));

    let mut bad_object = QuadKey::ObjectPredicates {
        namespace: 1,
        graph: 0,
        object: EncodedTerm::uid(3),
    }
    .to_bytes();
    bad_object[17] = 0x7f;
    assert!(matches!(
        QuadKey::parse(&bad_object).unwrap_err().0,
        QuadKeyErrorEnum::BadObject { .. }
    ));

    assert!(matches!(
        QuadKey::parse(&[0x42; 30]).unwrap_err().0,
        QuadKeyErrorEnum::UnknownKeyspace { .. }
    ));
    assert!(matches!(
        QuadKey::parse(&[]).unwrap_err().0,
        QuadKeyErrorEnum::Truncated { .. }
    ));

    let mut direction = QuadKey::CountIndex {
        namespace: 1,
        graph: 0,
        predicate: 1,
        direction: ultragraphdb_core_keys::CountDirection::Forward,
        count: 1,
    }
    .to_bytes();
    direction[25] = 7;
    assert!(matches!(
        QuadKey::parse(&direction).unwrap_err().0,
        QuadKeyErrorEnum::UnknownDirection { .. }
    ));

    let mut trailing = QuadKey::Statement {
        namespace: 1,
        graph: 0,
        statement: 1,
    }
    .to_bytes();
    trailing.push(1);
    assert!(matches!(
        QuadKey::parse(&trailing).unwrap_err().0,
        QuadKeyErrorEnum::TrailingBytes { .. }
    ));
}
