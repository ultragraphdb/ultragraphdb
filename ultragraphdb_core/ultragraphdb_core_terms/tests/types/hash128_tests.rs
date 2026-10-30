/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::Hash128;

#[test]
fn test_the_hash_is_blake2b_with_a_16_byte_digest() {
    // RFC 7693's test input "abc", BLAKE2b with nn = 16: Python's `hashlib.blake2b(b"abc", digest_size=16)`.
    let expected = [
        0xcf, 0x4a, 0xb7, 0x91, 0xc6, 0x2b, 0x8d, 0x2b, 0x21, 0x09, 0xc9, 0x02, 0x75, 0x28, 0x78,
        0x16,
    ];
    assert_eq!(Hash128::of(b"abc").as_bytes(), &expected);
    assert_eq!(Hash128::from_bytes(expected), Hash128::of(b"abc"));
    assert_ne!(Hash128::of(b"abc"), Hash128::of(b"abd"));
}

#[test]
fn test_the_hash_of_no_bytes_is_blake2b_of_the_empty_input() {
    // Python's `hashlib.blake2b(b"", digest_size=16)`.
    let expected = [
        0xca, 0xe6, 0x69, 0x41, 0xd9, 0xef, 0xbd, 0x40, 0x4e, 0x4d, 0x88, 0x75, 0x8e, 0xa6, 0x76,
        0x70,
    ];
    assert_eq!(Hash128::of(b"").as_bytes(), &expected);
}
