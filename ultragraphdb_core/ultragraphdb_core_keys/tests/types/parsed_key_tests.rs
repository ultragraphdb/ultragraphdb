/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::{
    BYTE_COUNT, BYTE_COUNT_REV, BYTE_DATA, BYTE_INDEX, BYTE_REVERSE, BYTE_SCHEMA, KeyError,
    ParsedKey, attr_in_root_namespace, count_key, data_key, index_key, parse, reverse_key,
    schema_key, split_key,
};

fn parsed(key: &[u8]) -> ParsedKey {
    parse(key).expect("parses")
}

#[test]
fn test_is_of_type() {
    let attr = attr_in_root_namespace("name");
    let cases: [(Vec<u8>, &[u8]); 5] = [
        (data_key(&attr, 1).unwrap(), &[BYTE_DATA]),
        (reverse_key(&attr, 1).unwrap(), &[BYTE_REVERSE]),
        (index_key(&attr, b"t").unwrap(), &[BYTE_INDEX]),
        (
            count_key(&attr, 1, false).unwrap(),
            &[BYTE_COUNT, BYTE_COUNT_REV],
        ),
        (
            count_key(&attr, 1, true).unwrap(),
            &[BYTE_COUNT, BYTE_COUNT_REV],
        ),
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
    let schema = parsed(&schema_key(&attr).unwrap());
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
        parsed(&data_key(&attr, 3).unwrap()).to_string(),
        "UID: 3, Attr: 0-name, Data key, prefix; 0, byte: 0"
    );
    assert_eq!(
        parsed(&index_key(&attr, &[1, 255]).unwrap()).to_string(),
        "UID: 0, Attr: 0-name, IsIndex: true, Term: [1 255]"
    );
    assert_eq!(
        parsed(&count_key(&attr, 9, true).unwrap()).to_string(),
        "UID: 0, Attr: 0-name, IsCount/Ref: true, Count: 9"
    );
    assert_eq!(
        parsed(&schema_key(&attr).unwrap()).to_string(),
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

#[test]
fn test_prefixes_of_a_split_key() {
    // Hand-built from x/keys.go: generateKey(prefix, attr, 1) and the last byte. The split prefix carries over to the
    // skip and data prefixes alone; the others take the default prefix.
    let attr = attr_in_root_namespace("ab");
    let pk = parsed(&split_key(&data_key(&attr, 3).unwrap(), 9).unwrap());
    let head = |prefix: u8, last: u8| vec![prefix, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, b'a', b'b', last];
    assert_eq!(pk.skip_predicate().unwrap(), head(4, 0xff));
    assert_eq!(pk.data_prefix().unwrap(), head(4, 0x00));
    assert_eq!(pk.index_prefix().unwrap(), head(0, 0x02));
    assert_eq!(pk.reverse_prefix().unwrap(), head(0, 0x04));
    assert_eq!(pk.count_prefix(false).unwrap(), head(0, 0x08));
    assert_eq!(pk.count_prefix(true).unwrap(), head(0, 0x0c));
}

#[test]
fn test_prefixes_of_an_unused_key_are_refused() {
    // An unused key parses to an empty attribute, on which Dgraph's generateKey panics past its split.
    let pk = parsed(&[0xff; 9]);
    let refused = KeyError::MissingNamespaceSeparator(String::new());
    assert_eq!(pk.skip_predicate(), Err(refused.clone()));
    assert_eq!(pk.data_prefix(), Err(refused.clone()));
    assert_eq!(pk.index_prefix(), Err(refused.clone()));
    assert_eq!(pk.reverse_prefix(), Err(refused.clone()));
    assert_eq!(pk.count_prefix(false), Err(refused.clone()));
    assert_eq!(pk.count_prefix(true), Err(refused));
    assert_eq!(
        pk.to_string(),
        "UID: 0, Attr: , Data key, prefix; 255, byte: 0"
    );
}
