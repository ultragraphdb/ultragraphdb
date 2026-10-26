/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `parse` of Dgraph's key layout: an unused prefix, bytes after a suffix, an attribute that is not UTF-8, a split key.

use ultragraphdb_core_keys::{
    BYTE_DATA, BYTE_UNUSED, KeyErrorEnum, attr_in_root_namespace, data_key, parse, split_key,
};

#[test]
fn test_parse_unused_prefix_is_empty() {
    let mut key = vec![BYTE_UNUSED];
    key.extend([0u8; 8]);
    let pk = parse(&key).expect("an unused key parses");
    assert_eq!(
        (pk.attr(), pk.uid(), pk.term(), pk.byte_type()),
        ("", 0, &[][..], 0)
    );
    assert!(!pk.is_data() && !pk.is_schema() && !pk.is_type() && !pk.has_start_uid());
}

#[test]
fn test_parse_ignores_bytes_after_unsplit_suffix() {
    let mut key = data_key(&attr_in_root_namespace("name"), 5).unwrap();
    key.extend([1, 2, 3]);
    let pk = parse(&key).expect("parses");
    assert_eq!(
        (pk.uid(), pk.byte_type(), pk.has_start_uid()),
        (5, BYTE_DATA, false)
    );
}

#[test]
fn test_parse_rejects_attribute_that_is_not_utf8() {
    // Dgraph's attributes are Go strings and may hold any byte; this port's are `String`s.
    let key = [
        0u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0xff, 0xfe, 0, 0, 0, 0, 0, 0, 0, 0, 1,
    ];
    let err = parse(&key).expect_err("not UTF-8");
    assert!(matches!(err.0, KeyErrorEnum::AttributeNotUtf8 { .. }));
    assert_eq!(
        err.to_string(),
        "Invalid UTF-8 attribute for key [0 2 255 254 0 0 0 0 0 0 0 0 1]"
    );
}

#[test]
fn test_parse_split_key_round_trip() {
    let key =
        split_key(&data_key(&attr_in_root_namespace("name"), 9).unwrap(), 1000).expect("splits");
    let pk = parse(&key).expect("parses");
    assert_eq!(
        (pk.uid(), pk.start_uid(), pk.has_start_uid()),
        (9, 1000, true)
    );
}
