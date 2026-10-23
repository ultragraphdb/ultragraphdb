/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `parse` against Dgraph's own parse of every golden key and of malformed keys, error texts included.

use std::panic;

use ultragraphdb_core_keys::{
    BYTE_DATA, BYTE_UNUSED, KeyErrorEnum, ParsedKey, attr_in_root_namespace, data_key, parse,
    split_key,
};

use crate::golden::{KeyCase, ParsedCase, compare, golden, parse_errors, unhex};

fn check_parsed(input: &str, expected: &ParsedCase, pk: &ParsedKey) {
    assert_eq!(pk.attr(), expected.attr, "{input}");
    assert_eq!(pk.uid(), expected.uid, "{input}");
    assert_eq!(pk.start_uid(), expected.start_uid, "{input}");
    assert_eq!(pk.term(), unhex(&expected.term), "{input}");
    assert_eq!(pk.count(), expected.count, "{input}");
    assert_eq!(pk.byte_type(), expected.byte_type, "{input}");
    assert_eq!(pk.has_start_uid(), expected.has_start_uid, "{input}");
    assert_eq!(pk.is_data(), expected.is_data, "{input}");
    assert_eq!(pk.is_reverse(), expected.is_reverse, "{input}");
    assert_eq!(pk.is_count(), expected.is_count, "{input}");
    assert_eq!(pk.is_count_rev(), expected.is_count_rev, "{input}");
    assert_eq!(
        pk.is_count_or_count_rev(),
        expected.is_count || expected.is_count_rev,
        "{input}"
    );
    assert_eq!(pk.is_index(), expected.is_index, "{input}");
    assert_eq!(pk.is_schema(), expected.is_schema, "{input}");
    assert_eq!(pk.is_type(), expected.is_type, "{input}");
    assert_eq!(pk.to_string(), expected.string, "{input}");
    if !expected.skip_predicate.is_empty() {
        compare(input, &expected.skip_predicate, &pk.skip_predicate());
        compare(input, &expected.data_prefix, &pk.data_prefix());
        compare(input, &expected.index_prefix, &pk.index_prefix());
        compare(input, &expected.reverse_prefix, &pk.reverse_prefix());
        compare(input, &expected.count_prefix, &pk.count_prefix(false));
        compare(input, &expected.count_rev_prefix, &pk.count_prefix(true));
    }
}

fn check(input: &str, key: &[u8], parsed: Option<&ParsedCase>, error: &str, panics: bool) {
    if panics {
        assert!(
            panic::catch_unwind(|| parse(key)).is_err(),
            "{input}: Dgraph panics"
        );
        return;
    }
    match (parse(key), parsed) {
        (Ok(pk), Some(expected)) => check_parsed(input, expected, &pk),
        (Err(e), None) => assert_eq!(e.to_string(), error, "{input}"),
        (actual, _) => panic!("{input}: expected {parsed:?} / {error:?}, got {actual:?}"),
    }
}

#[test]
fn test_golden_parse() {
    let golden = golden::<KeyCase>("keys");
    let mut with_prefixes = 0;
    for case in golden.cases() {
        let input = format!(
            "{} {} {:?} {}",
            case.kind, case.namespace, case.attr, case.key
        );
        check(
            &input,
            &unhex(&case.key),
            case.parsed.as_ref(),
            &case.error,
            case.panics,
        );
        with_prefixes += usize::from(
            case.parsed
                .as_ref()
                .is_some_and(|p| !p.skip_predicate.is_empty()),
        );
    }
    assert!(with_prefixes > 100);
}

#[test]
fn test_golden_parse_errors() {
    let cases = parse_errors().parse;
    assert!(cases.len() > 30);
    for case in &cases {
        check(
            &case.name,
            &unhex(&case.key),
            case.parsed.as_ref(),
            &case.error,
            case.panics,
        );
    }
}

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
    let mut key = data_key(&attr_in_root_namespace("name"), 5);
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
    let key = split_key(&data_key(&attr_in_root_namespace("name"), 9), 1000).expect("splits");
    let pk = parse(&key).expect("parses");
    assert_eq!(
        (pk.uid(), pk.start_uid(), pk.has_start_uid()),
        (9, 1000, true)
    );
}
