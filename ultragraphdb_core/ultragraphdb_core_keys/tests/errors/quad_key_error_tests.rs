/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_core_ids::IdError;
use ultragraphdb_core_keys::{QuadKeyError, QuadKeyErrorEnum};
use ultragraphdb_core_terms::TermError;

#[test]
fn test_each_constructor_builds_its_variant() {
    let key = vec![0x10, 0xab];
    let cases = [
        (
            QuadKeyError::truncated(&key),
            QuadKeyErrorEnum::Truncated { key: key.clone() },
        ),
        (
            QuadKeyError::trailing_bytes(&key),
            QuadKeyErrorEnum::TrailingBytes { key: key.clone() },
        ),
        (
            QuadKeyError::unknown_keyspace(&key),
            QuadKeyErrorEnum::UnknownKeyspace { key: key.clone() },
        ),
        (
            QuadKeyError::invalid_id(&key, "subject", IdError::zero_sequence()),
            QuadKeyErrorEnum::InvalidId {
                key: key.clone(),
                field: "subject",
                error: IdError::zero_sequence(),
            },
        ),
        (
            QuadKeyError::unknown_direction(&key),
            QuadKeyErrorEnum::UnknownDirection { key: key.clone() },
        ),
        (
            QuadKeyError::bad_object(&key, TermError::unknown_tag(0x7f)),
            QuadKeyErrorEnum::BadObject {
                key: key.clone(),
                error: TermError::unknown_tag(0x7f),
            },
        ),
        (
            QuadKeyError::vector_object(&key),
            QuadKeyErrorEnum::VectorObject { key: key.clone() },
        ),
        (
            QuadKeyError::not_a_list(&key),
            QuadKeyErrorEnum::NotAList { key: key.clone() },
        ),
        (
            QuadKeyError::group_range(&key, 0x4_0000),
            QuadKeyErrorEnum::GroupRange {
                key: key.clone(),
                group: 0x4_0000,
            },
        ),
        (
            QuadKeyError::fingerprint_start(&key),
            QuadKeyErrorEnum::FingerprintStart { key: key.clone() },
        ),
        (
            QuadKeyError::unknown_kind(&key, 0x07),
            QuadKeyErrorEnum::UnknownKind {
                key: key.clone(),
                kind: 0x07,
            },
        ),
    ];
    for (error, variant) in cases {
        assert_eq!(error.0, variant);
    }
}

#[test]
fn test_each_error_names_the_keys_bytes() {
    let key = [0x10, 0xab];
    let cases = [
        (
            QuadKeyError::truncated(&key),
            "Key 10ab ends before its fields do",
        ),
        (
            QuadKeyError::trailing_bytes(&key),
            "Key 10ab has bytes after its fields",
        ),
        (
            QuadKeyError::unknown_keyspace(&key),
            "Key 10ab belongs to no keyspace",
        ),
        (
            QuadKeyError::invalid_id(&key, "subject", IdError::zero_sequence()),
            "Key 10ab has an invalid subject: invalid system ID: its sequence is 0",
        ),
        (
            QuadKeyError::unknown_direction(&key),
            "Count key 10ab has an unknown direction",
        ),
        (
            QuadKeyError::bad_object(&key, TermError::unknown_tag(0x7f)),
            "Key 10ab has an unreadable object: Unknown term tag 0x7f",
        ),
        (
            QuadKeyError::vector_object(&key),
            "Key 10ab names a vector as its object",
        ),
        (
            QuadKeyError::not_a_list(&key),
            "Key 10ab is not a list's and does not split",
        ),
        (
            QuadKeyError::group_range(&key, 0x4_0000),
            "Mark key 10ab has bucket group 262144, past 18 bits",
        ),
        (
            QuadKeyError::fingerprint_start(&key),
            "Value-index key 10ab splits at an ID alone, not a fingerprint",
        ),
        (
            QuadKeyError::unknown_kind(&key, 0x07),
            "Key 10ab holds the kind byte 0x07, which names no split start or dictionary entry",
        ),
    ];
    for (error, text) in cases {
        assert_eq!(error.to_string(), text);
        assert!(error.source().is_none());
    }
}
