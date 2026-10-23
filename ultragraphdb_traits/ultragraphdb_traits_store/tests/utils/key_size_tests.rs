/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreErrorEnum;
use ultragraphdb_traits_store::{
    MAX_STORED_KEY_SIZE, MAX_USER_KEY_SIZE, check_key_size, stored_key_size,
};

#[test]
fn test_the_stored_size_counts_escapes_terminator_and_version() {
    assert_eq!(stored_key_size(b""), 10);
    assert_eq!(stored_key_size(b"abc"), 13);
    assert_eq!(stored_key_size(&[0, 1, 0]), 15);
}

#[test]
fn test_a_user_key_over_badgers_limit_is_refused_with_badgers_text() {
    assert!(check_key_size(&vec![7; MAX_USER_KEY_SIZE]).is_ok());
    let err = check_key_size(&vec![7; MAX_USER_KEY_SIZE + 1]).unwrap_err();
    match &err.0 {
        StoreErrorEnum::KeyTooLarge { size, limit, head } => {
            assert_eq!((*size, *limit, head.len()), (65_001, 65_000, 1024));
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(
        err.to_string()
            .starts_with("Key with size 65001 exceeded 65000 limit. Key:\n00000000  07 07")
    );
}

#[test]
fn test_a_stored_key_over_fjalls_limit_is_refused_with_its_escaped_head() {
    let err = check_key_size(&vec![0; 40_000]).unwrap_err();
    match &err.0 {
        StoreErrorEnum::KeyTooLarge { size, limit, head } => {
            assert_eq!((*size, *limit), (80_010, MAX_STORED_KEY_SIZE));
            assert_eq!(head.len(), 1024);
            assert!(head.chunks(2).all(|pair| pair == [0x00, 0xFF]));
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(
        err.to_string()
            .starts_with("Key with size 80010 exceeded 65535 limit")
    );
}

#[test]
fn test_the_stored_limit_is_inclusive() {
    // 32,762 zero bytes and a one are 65,535 bytes stored; one zero byte more escapes past the limit.
    let mut key = vec![0; 32_762];
    key.push(1);
    assert_eq!(stored_key_size(&key), MAX_STORED_KEY_SIZE);
    assert!(check_key_size(&key).is_ok());
    assert!(check_key_size(&vec![0; 32_763]).is_err());
}
