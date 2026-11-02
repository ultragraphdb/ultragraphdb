/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::Hash128;

#[test]
fn test_the_hash_is_sha256_truncated_to_16_bytes() {
    // FIPS 180-2's test input "abc", SHA-256's first 16 bytes: Python's `hashlib.sha256(b"abc").digest()[:16]`.
    let expected = [
        0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae, 0x22,
        0x23,
    ];
    assert_eq!(Hash128::of(b"abc").as_bytes(), &expected);
    assert_eq!(Hash128::from_bytes(expected), Hash128::of(b"abc"));
    assert_ne!(Hash128::of(b"abc"), Hash128::of(b"abd"));
}

#[test]
fn test_the_hash_of_no_bytes_is_sha256_of_the_empty_input() {
    // Python's `hashlib.sha256(b"").digest()[:16]`.
    let expected = [
        0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f, 0xb9,
        0x24,
    ];
    assert_eq!(Hash128::of(b"").as_bytes(), &expected);
}
