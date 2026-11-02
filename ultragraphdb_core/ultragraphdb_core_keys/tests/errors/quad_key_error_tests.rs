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
            QuadKeyError::zero_uid(&key, "subject"),
            "Key 10ab has a subject of 0",
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
    ];
    for (error, text) in cases {
        assert_eq!(error.to_string(), text);
    }
}
