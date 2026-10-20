/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::{
    BYTE_COUNT, BYTE_COUNT_REV, BYTE_DATA, BYTE_INDEX, BYTE_REVERSE, BYTE_SCHEMA, ParsedKey,
    attr_in_root_namespace, count_key, data_key, index_key, parse, reverse_key, schema_key,
    split_key,
};

fn parsed(key: &[u8]) -> ParsedKey {
    parse(key).expect("parses")
}

#[test]
fn test_is_of_type() {
    let attr = attr_in_root_namespace("name");
    let cases: [(Vec<u8>, &[u8]); 5] = [
        (data_key(&attr, 1), &[BYTE_DATA]),
        (reverse_key(&attr, 1), &[BYTE_REVERSE]),
        (index_key(&attr, b"t"), &[BYTE_INDEX]),
        (count_key(&attr, 1, false), &[BYTE_COUNT, BYTE_COUNT_REV]),
        (count_key(&attr, 1, true), &[BYTE_COUNT, BYTE_COUNT_REV]),
    ];
    for (key, types) in &cases {
        let pk = parsed(key);
        let split = parsed(&split_key(key, 7).expect("splits"));
        for typ in [
            BYTE_DATA,
            BYTE_REVERSE,
            BYTE_INDEX,
            BYTE_COUNT,
            BYTE_COUNT_REV,
            BYTE_SCHEMA,
            0x55,
        ] {
            assert_eq!(
                pk.is_of_type(typ),
                types.contains(&typ),
                "{pk} is of type {typ}"
            );
            assert_eq!(
                split.is_of_type(typ),
                types.contains(&typ),
                "{split} is of type {typ}"
            );
        }
    }
    let schema = parsed(&schema_key(&attr));
    assert!(
        [BYTE_DATA, BYTE_REVERSE, BYTE_INDEX, BYTE_COUNT, BYTE_SCHEMA]
            .iter()
            .all(|&t| !schema.is_of_type(t))
    );
}

#[test]
fn test_display() {
    let attr = attr_in_root_namespace("name");
    assert_eq!(
        parsed(&data_key(&attr, 3)).to_string(),
        "UID: 3, Attr: 0-name, Data key, prefix; 0, byte: 0"
    );
    assert_eq!(
        parsed(&index_key(&attr, &[1, 255])).to_string(),
        "UID: 0, Attr: 0-name, IsIndex: true, Term: [1 255]"
    );
    assert_eq!(
        parsed(&count_key(&attr, 9, true)).to_string(),
        "UID: 0, Attr: 0-name, IsCount/Ref: true, Count: 9"
    );
    assert_eq!(
        parsed(&schema_key(&attr)).to_string(),
        "UID: 0, Attr: 0-name, Data key, prefix; 1, byte: 0"
    );
}

#[test]
fn test_default_is_the_zero_key() {
    let pk = ParsedKey::default();
    assert_eq!(
        (
            pk.attr(),
            pk.uid(),
            pk.start_uid(),
            pk.count(),
            pk.byte_type()
        ),
        ("", 0, 0, 0, 0)
    );
    assert!(pk.term().is_empty() && !pk.has_start_uid());
    // Go's zero ParsedKey has the default prefix, so it reads as a data key.
    assert!(pk.is_data());
}
