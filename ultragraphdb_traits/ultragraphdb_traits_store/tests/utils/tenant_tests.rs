/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::{TENANT_OFFSET, key_tenant};

/// A key of prefix byte `prefix`, tenant bytes `tenant` and `rest` after them.
fn key(prefix: u8, tenant: [u8; 8], rest: &[u8]) -> Vec<u8> {
    let mut key = vec![prefix];
    key.extend_from_slice(&tenant);
    key.extend_from_slice(rest);
    key
}

#[test]
fn test_the_tenant_is_the_eight_big_endian_bytes_after_the_prefix_byte() {
    // Dgraph's x.NamespaceOffset is 1; Badger's isBanned reads y.BytesToU64, binary.BigEndian.Uint64.
    assert_eq!(TENANT_OFFSET, 1);
    let cases: &[([u8; 8], u64)] = &[
        ([0; 8], 0),
        ([0, 0, 0, 0, 0, 0, 0, 5], 5),
        ([1, 2, 3, 4, 5, 6, 7, 8], 0x0102_0304_0506_0708),
        ([0x80, 0, 0, 0, 0, 0, 0, 0], 1 << 63),
        ([0xFF; 8], u64::MAX),
    ];
    for &(tenant, expected) in cases {
        assert_eq!(
            key_tenant(&key(0x00, tenant, b"\x00\x04name")),
            Some(expected),
            "{tenant:?}"
        );
    }
}

#[test]
fn test_the_prefix_byte_and_the_bytes_after_the_tenant_do_not_count() {
    let tenant = [0, 0, 0, 0, 0, 0, 0, 9];
    assert_eq!(key_tenant(&key(0xFF, tenant, b"x")), Some(9));
    assert_eq!(key_tenant(&key(0x04, tenant, &[0xFF; 64])), Some(9));
}

#[test]
fn test_a_key_no_longer_than_its_tenant_has_none() {
    // Badger's isBanned skips keys of at most NamespaceOffset+8 bytes: 9 here; 10 is the shortest with one.
    assert_eq!(key_tenant(&key(0x00, [0, 0, 0, 0, 0, 0, 0, 5], b"")), None);
    assert_eq!(
        key_tenant(&key(0x00, [0, 0, 0, 0, 0, 0, 0, 5], b"a")),
        Some(5)
    );
    for len in 0..=9 {
        assert_eq!(key_tenant(&vec![0xFF; len]), None, "key of {len} bytes");
    }
}
