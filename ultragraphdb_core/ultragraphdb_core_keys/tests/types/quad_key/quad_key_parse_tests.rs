/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{Literal, NamedNode};
use ultragraphdb_core_ids::{IdError, IdErrorEnum, SystemId};
use ultragraphdb_core_keys::{CountDirection, DEFAULT_GRAPH, Graph, QuadKey, QuadKeyErrorEnum};
use ultragraphdb_core_terms::{EncodedTerm, FLOAT32_VECTOR, TermError};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::types::quad_key::quad_key_tests::{every_keyspace, graph, id};

/// The length of a key's fixed head: the keyspace byte, the namespace and the graph.
const HEAD: usize = 1 + 8 + 16;

#[test]
fn test_every_proper_prefix_of_every_key_is_refused_unless_a_shorter_token_makes_it_a_key()
-> TestResult {
    for key in every_keyspace()? {
        let bytes = key.to_bytes();
        for end in 0..bytes.len() {
            let prefix = bytes.get(..end).context("a prefix")?;
            let result = std::panic::catch_unwind(|| QuadKey::parse(prefix))
                .map_err(|_| TestError::new("parse panicked"))?;
            if let Ok(parsed) = result {
                // A value-index key's token is its last field and may be empty: from the tokenizer byte on, a
                // prefix is a key with a shorter token.
                assert!(
                    matches!(parsed, (QuadKey::ValueIndex { .. }, None) if end > HEAD + 16),
                    "{prefix:?} of {key:?} parsed as {parsed:?}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn test_a_cut_inside_any_id_is_truncated_naming_the_key() -> TestResult {
    // The forward key's predicate starts at 25 and its subject at 41; a cut inside either leaves a field short.
    let key = QuadKey::Forward {
        namespace: 1,
        graph: graph()?,
        predicate: id(1, 9, 9)?,
        subject: id(1, 5, 5)?,
    }
    .to_bytes();
    for end in (HEAD + 1..HEAD + 16).chain(HEAD + 17..HEAD + 32) {
        let cut = key.get(..end).context("a cut")?;
        assert_eq!(
            QuadKey::parse(cut).map_err(|e| e.0),
            Err(QuadKeyErrorEnum::Truncated { key: cut.to_vec() }),
            "{end}"
        );
    }
    Ok(())
}

/// The ways an ID's 16 bytes are invalid, each written over a valid ID, with the refusal it draws.
fn invalid_ids() -> [([u8; 16], IdError); 3] {
    let mut zero_sequence = [0u8; 16];
    zero_sequence[1] = 1;
    let mut zero_prefix = [0u8; 16];
    zero_prefix[9] = 1;
    let mut reserved = zero_sequence;
    reserved[9] = 1;
    reserved[15] = 1;
    [
        (zero_sequence, IdError::zero_sequence()),
        (zero_prefix, IdError::zero_type_prefix()),
        (reserved, IdError::reserved_bits(1)),
    ]
}

/// Each key with the offset of an ID it holds and the field naming it.
fn id_fields() -> TestResult<Vec<(QuadKey, usize, &'static str)>> {
    let keys = every_keyspace()?;
    let key = |i: usize| keys.get(i).cloned().context("a key of every_keyspace");
    Ok(vec![
        (key(0)?, 10, "id"),
        (key(2)?, 9, "graph"),
        (key(1)?, HEAD, "predicate"),
        (key(1)?, HEAD + 16, "subject"),
        (key(4)?, HEAD, "subject"),
        (key(9)?, HEAD + 17, "node"),
        (key(10)?, HEAD, "statement ID"),
        (key(11)?, HEAD, "predicate"),
        (key(12)?, 9, "entity"),
        (key(13)?, 9, "entity"),
        (key(14)?, 10, "ID"),
    ])
}

#[test]
fn test_an_invalid_id_is_refused_in_every_field_naming_it() -> TestResult {
    for (key, at, field) in id_fields()? {
        for (bytes, error) in invalid_ids() {
            let mut corrupt = key.to_bytes();
            corrupt
                .get_mut(at..at + 16)
                .context("the ID's bytes")?
                .copy_from_slice(&bytes);
            assert_eq!(
                QuadKey::parse(&corrupt).map_err(|e| e.0),
                Err(QuadKeyErrorEnum::InvalidId {
                    key: corrupt.clone(),
                    field,
                    error
                }),
                "{key:?} {field}"
            );
        }
    }
    Ok(())
}

#[test]
fn test_16_zero_bytes_are_the_default_graph_and_nothing_else() -> TestResult {
    // The default graph's 16 zero bytes read back as the default graph; in an ID field they are a zero prefix.
    let key = QuadKey::Statement {
        namespace: 1,
        graph: DEFAULT_GRAPH,
        statement: id(2, 1, 1)?,
    };
    assert_eq!(
        QuadKey::parse(&key.to_bytes())?.0.graph(),
        Some(Graph::Default)
    );
    let mut zero_statement = key.to_bytes();
    zero_statement
        .get_mut(HEAD..)
        .context("the statement ID")?
        .fill(0);
    assert!(matches!(
        QuadKey::parse(&zero_statement).map_err(|e| e.0),
        Err(QuadKeyErrorEnum::InvalidId {
            field: "statement ID",
            error: IdError(IdErrorEnum::ReservedBits { .. } | IdErrorEnum::ZeroTypePrefix),
            ..
        })
    ));
    Ok(())
}

#[test]
fn test_a_mark_group_past_18_bits_is_refused() -> TestResult {
    let key = QuadKey::Mark {
        namespace: 1,
        entity: id(1, 1, 1)?,
        group: 0,
    };
    // 2^18, the first group past 18 bits, and 2^24 - 1, the largest 3 bytes hold.
    for (bytes, group) in [
        ([0x04u8, 0, 0], 0x4_0000u32),
        ([0xFF, 0xFF, 0xFF], 0xFF_FFFF),
    ] {
        let mut corrupt = key.to_bytes();
        corrupt
            .get_mut(25..28)
            .context("the group's bytes")?
            .copy_from_slice(&bytes);
        assert_eq!(
            QuadKey::parse(&corrupt).map_err(|e| e.0),
            Err(QuadKeyErrorEnum::GroupRange {
                key: corrupt.clone(),
                group
            })
        );
    }
    Ok(())
}

#[test]
fn test_unreadable_objects_unknown_keyspaces_and_directions_are_refused() -> TestResult {
    let vector = EncodedTerm::literal(
        Literal::new_typed_literal("[1]", NamedNode::new_unchecked(FLOAT32_VECTOR)).as_ref(),
        |_| Err::<SystemId, _>(TermError::malformed(Vec::new(), "no datatype ID is needed")),
    )?
    .into_term();
    let key = QuadKey::Reverse {
        namespace: 1,
        graph: DEFAULT_GRAPH,
        predicate: id(1, 1, 1)?,
        object: vector,
    };
    assert!(matches!(
        QuadKey::parse(&key.to_bytes()).map_err(|e| e.0),
        Err(QuadKeyErrorEnum::VectorObject { .. })
    ));

    let mut bad_object = QuadKey::ObjectPredicates {
        namespace: 1,
        graph: DEFAULT_GRAPH,
        object: EncodedTerm::id(id(1, 3, 3)?),
    }
    .to_bytes();
    // The object's tag follows the head.
    *bad_object.get_mut(HEAD).context("the object's tag")? = 0x7f;
    assert!(matches!(
        QuadKey::parse(&bad_object).map_err(|e| e.0),
        Err(QuadKeyErrorEnum::BadObject { .. })
    ));

    assert!(matches!(
        QuadKey::parse(&[0x42; 60]).map_err(|e| e.0),
        Err(QuadKeyErrorEnum::UnknownKeyspace { .. })
    ));
    assert_eq!(
        QuadKey::parse(&[]).map_err(|e| e.0),
        Err(QuadKeyErrorEnum::Truncated { key: Vec::new() })
    );

    let mut direction = QuadKey::CountIndex {
        namespace: 1,
        graph: DEFAULT_GRAPH,
        predicate: id(1, 1, 1)?,
        direction: CountDirection::Forward,
        count: 1,
    }
    .to_bytes();
    // The direction follows the head and the predicate.
    *direction.get_mut(HEAD + 16).context("the direction")? = 7;
    assert!(matches!(
        QuadKey::parse(&direction).map_err(|e| e.0),
        Err(QuadKeyErrorEnum::UnknownDirection { .. })
    ));
    Ok(())
}

#[test]
fn test_a_byte_after_any_key_but_a_value_index_key_is_refused() -> TestResult {
    for key in every_keyspace()? {
        let mut trailing = key.to_bytes();
        trailing.push(1);
        let result = QuadKey::parse(&trailing).map_err(|e| e.0);
        if matches!(key, QuadKey::ValueIndex { .. }) {
            // The byte becomes the token's last.
            assert!(result.is_ok(), "{key:?}");
        } else if matches!(
            key,
            QuadKey::Reverse { .. } | QuadKey::ObjectPredicates { .. }
        ) {
            // A byte after the object ends the object where its tag says, then trails.
            assert!(
                matches!(result, Err(QuadKeyErrorEnum::TrailingBytes { .. })),
                "{key:?}: {result:?}"
            );
        } else {
            assert_eq!(
                result,
                Err(QuadKeyErrorEnum::TrailingBytes {
                    key: trailing.clone()
                }),
                "{key:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn test_an_object_that_is_no_valid_id_is_refused() -> TestResult {
    // A reverse key whose object, an IRI's tag and 16 bytes, has sequence 0: the key names no entity.
    let key = QuadKey::Reverse {
        namespace: 1,
        graph: DEFAULT_GRAPH,
        predicate: id(1, 1, 1)?,
        object: EncodedTerm::id(id(1, 3, 3)?),
    };
    let mut bytes = key.to_bytes();
    let at = bytes.len() - 16;
    // The object's sequence, bytes 2 to 9 of its ID.
    bytes
        .get_mut(at + 2..at + 10)
        .context("the sequence")?
        .fill(0);
    assert_eq!(
        QuadKey::parse(&bytes).map_err(|e| e.0),
        Err(QuadKeyErrorEnum::InvalidId {
            key: bytes.clone(),
            field: "object",
            error: IdError::zero_sequence()
        })
    );
    Ok(())
}
