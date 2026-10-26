/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::QuadKeyError;
use ultragraphdb_core_terms::TermError;

#[test]
fn test_each_error_names_the_keys_bytes() {
    let key = [0x10, 0xab];
    let cases = [
        (
            QuadKeyError::Truncated(&key),
            "Key 10ab ends before its fields do",
        ),
        (
            QuadKeyError::TrailingBytes(&key),
            "Key 10ab has bytes after its fields",
        ),
        (
            QuadKeyError::UnknownKeyspace(&key),
            "Key 10ab belongs to no keyspace",
        ),
        (
            QuadKeyError::ZeroUid(&key, "subject"),
            "Key 10ab has a subject of 0",
        ),
        (
            QuadKeyError::UnknownDirection(&key),
            "Count key 10ab has an unknown direction",
        ),
        (
            QuadKeyError::BadObject(&key, TermError::UnknownTag(0x7f)),
            "Key 10ab has an unreadable object: Unknown term tag 0x7f",
        ),
        (
            QuadKeyError::VectorObject(&key),
            "Key 10ab names a vector as its object",
        ),
        (
            QuadKeyError::NotAList(&key),
            "Key 10ab is not a list's and does not split",
        ),
    ];
    for (error, text) in cases {
        assert_eq!(error.to_string(), text);
    }
}
