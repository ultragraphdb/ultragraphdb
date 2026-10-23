/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::{NAMESPACE_OFFSET, key_namespace};

#[test]
fn test_the_namespace_is_the_eight_bytes_after_the_prefix_byte() {
    let mut key = vec![0x00];
    key.extend_from_slice(&5u64.to_be_bytes());
    key.extend_from_slice(b"\x00\x04name");
    assert_eq!(NAMESPACE_OFFSET, 1);
    assert_eq!(key_namespace(&key), Some(5));
}

#[test]
fn test_a_key_no_longer_than_its_namespace_has_none() {
    // Badger's isBanned skips keys of at most NamespaceOffset+8 bytes.
    let mut key = vec![0x00];
    key.extend_from_slice(&5u64.to_be_bytes());
    assert_eq!(key_namespace(&key), None);
    assert_eq!(key_namespace(b"short"), None);
}
