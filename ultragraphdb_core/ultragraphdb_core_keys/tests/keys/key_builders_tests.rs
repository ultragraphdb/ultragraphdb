/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/keys_test.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_keys::{
    BYTE_SPLIT, KeyError, KeyErrorEnum, attr_in_root_namespace, count_key, data_key, data_prefix,
    index_key, index_key_after_all_terms, namespace_attr, parse, parse_attr, predicate_prefix,
    reverse_key, schema_key, schema_prefix, split_key, type_key, type_prefix,
};

use crate::golden::{KeyCase, compare, golden, parse_errors, unhex};

fn build(case: &KeyCase) -> Vec<u8> {
    let attr = namespace_attr(case.namespace, &case.attr);
    let (kind, split) = match case.kind.strip_prefix("split_") {
        Some(kind) => (kind, true),
        None => (case.kind.as_str(), false),
    };
    let key = match kind {
        "data" => data_key(&attr, case.uid).unwrap(),
        "reverse" => reverse_key(&attr, case.uid).unwrap(),
        "index" => index_key(&attr, &unhex(&case.term)).unwrap(),
        "count" => count_key(&attr, case.count, false).unwrap(),
        "count_rev" => count_key(&attr, case.count, true).unwrap(),
        "schema" => schema_key(&attr).unwrap(),
        "type" => type_key(&attr).unwrap(),
        "index_after_all_terms" => index_key_after_all_terms(&attr).unwrap(),
        "predicate_prefix" => predicate_prefix(&attr).unwrap(),
        "data_prefix" => data_prefix(case.namespace),
        "schema_prefix" => schema_prefix(),
        "type_prefix" => type_prefix(),
        other => panic!("unknown golden key kind {other}"),
    };
    if split {
        split_key(&key, case.start_uid).expect("a default-prefix key splits")
    } else {
        key
    }
}

#[test]
fn test_golden_keys() {
    let golden = golden::<KeyCase>("keys");
    assert!(golden.cases().len() > 1000);
    for case in golden.cases() {
        compare(
            &format!("{} {} {:?}", case.kind, case.namespace, case.attr),
            &case.key,
            &build(case),
        );
    }
}

#[test]
fn test_golden_split_key() {
    for case in parse_errors().split {
        let actual = split_key(&unhex(&case.key), case.start_uid);
        if case.error.is_empty() {
            compare(&case.name, &case.split, &actual.expect("splits"));
        } else {
            assert_eq!(
                actual.expect_err(&case.name).to_string(),
                case.error,
                "{}",
                case.name
            );
        }
    }
}

#[test]
fn test_data_key() {
    // key with uid = 0 is invalid
    let key = data_key(&attr_in_root_namespace("bad uid"), 0).unwrap();
    assert!(parse(&key).is_err());

    for uid in 1..1001u64 {
        // Use the uid to derive the attribute so it has variable length and the test
        // can verify that multiple sizes of attr work correctly.
        let sattr = format!("attr:{uid}");
        let key = data_key(&attr_in_root_namespace(&sattr), uid).unwrap();
        let pk = parse(&key).expect("parses");

        assert!(pk.is_data());
        assert_eq!(sattr, parse_attr(pk.attr()).unwrap());
        assert_eq!(uid, pk.uid());
        assert_eq!(0, pk.start_uid());
    }

    let mut keys: Vec<Vec<u8>> = (1..=1024u64)
        .rev()
        .map(|uid| data_key(&attr_in_root_namespace("testing.key"), uid).unwrap())
        .collect();
    // Test that sorting is as expected.
    keys.sort();
    for (i, key) in keys.iter().enumerate() {
        let exp = data_key(&attr_in_root_namespace("testing.key"), i as u64 + 1).unwrap();
        assert_eq!(&exp, key);
    }
}

#[test]
fn test_parse_data_key_with_start_uid() {
    let start_uid = u64::MAX;
    for uid in 1..1001u64 {
        let sattr = format!("attr:{uid}");
        let key = data_key(&attr_in_root_namespace(&sattr), uid).unwrap();
        let key = split_key(&key, start_uid).expect("splits");
        let pk = parse(&key).expect("parses");

        assert!(pk.is_data());
        assert_eq!(sattr, parse_attr(pk.attr()).unwrap());
        assert_eq!(uid, pk.uid());
        assert!(pk.has_start_uid());
        assert_eq!(start_uid, pk.start_uid());
    }
}

#[test]
fn test_index_key() {
    for uid in 0..1001 {
        let sattr = format!("attr:{uid}");
        let sterm = format!("term:{uid}");

        let key = index_key(&attr_in_root_namespace(&sattr), sterm.as_bytes()).unwrap();
        let pk = parse(&key).expect("parses");

        assert!(pk.is_index());
        assert_eq!(sattr, parse_attr(pk.attr()).unwrap());
        assert_eq!(sterm.as_bytes(), pk.term());
    }
}

#[test]
fn test_index_key_with_start_uid() {
    let start_uid = u64::MAX;
    for uid in 0..1001 {
        let sattr = format!("attr:{uid}");
        let sterm = format!("term:{uid}");

        let key = index_key(&attr_in_root_namespace(&sattr), sterm.as_bytes()).unwrap();
        let key = split_key(&key, start_uid).expect("splits");
        let pk = parse(&key).expect("parses");

        assert!(pk.is_index());
        assert_eq!(sattr, parse_attr(pk.attr()).unwrap());
        assert_eq!(sterm.as_bytes(), pk.term());
        assert!(pk.has_start_uid());
        assert_eq!(start_uid, pk.start_uid());
    }
}

#[test]
fn test_reverse_key() {
    for uid in 1..1001u64 {
        let sattr = format!("attr:{uid}");

        let key = reverse_key(&attr_in_root_namespace(&sattr), uid).unwrap();
        let pk = parse(&key).expect("parses");

        assert!(pk.is_reverse());
        assert_eq!(sattr, parse_attr(pk.attr()).unwrap());
        assert_eq!(uid, pk.uid());
    }
}

#[test]
fn test_reverse_key_with_start_uid() {
    let start_uid = u64::MAX;
    for uid in 1..1001u64 {
        let sattr = format!("attr:{uid}");

        let key = reverse_key(&attr_in_root_namespace(&sattr), uid).unwrap();
        let key = split_key(&key, start_uid).expect("splits");
        let pk = parse(&key).expect("parses");

        assert!(pk.is_reverse());
        assert_eq!(sattr, parse_attr(pk.attr()).unwrap());
        assert_eq!(uid, pk.uid());
        assert!(pk.has_start_uid());
        assert_eq!(start_uid, pk.start_uid());
    }
}

#[test]
fn test_count_key() {
    for count in 0..1001u32 {
        let sattr = format!("attr:{count}");

        let key = count_key(&attr_in_root_namespace(&sattr), count, true).unwrap();
        let pk = parse(&key).expect("parses");

        assert!(pk.is_count_or_count_rev());
        assert_eq!(sattr, parse_attr(pk.attr()).unwrap());
        assert_eq!(count, pk.count());
    }
}

#[test]
fn test_count_key_with_start_uid() {
    let start_uid = u64::MAX;
    for count in 0..1001u32 {
        let sattr = format!("attr:{count}");

        let key = count_key(&attr_in_root_namespace(&sattr), count, true).unwrap();
        let key = split_key(&key, start_uid).expect("splits");
        let pk = parse(&key).expect("parses");

        assert!(pk.is_count_or_count_rev());
        assert_eq!(sattr, parse_attr(pk.attr()).unwrap());
        assert_eq!(count, pk.count());
        assert!(pk.has_start_uid());
        assert_eq!(start_uid, pk.start_uid());
    }
}

#[test]
fn test_schema_key() {
    for uid in 0..1001 {
        let sattr = format!("attr:{uid}");

        let key = schema_key(&attr_in_root_namespace(&sattr)).unwrap();
        let pk = parse(&key).expect("parses");

        assert!(pk.is_schema());
        assert_eq!(sattr, parse_attr(pk.attr()).unwrap());
    }
}

#[test]
fn test_type_key() {
    for uid in 0..1001 {
        let sattr = format!("attr:{uid}");

        let key = type_key(&attr_in_root_namespace(&sattr)).unwrap();
        let pk = parse(&key).expect("parses");

        assert!(pk.is_type());
        assert_eq!(sattr, parse_attr(pk.attr()).unwrap());
    }
}

#[test]
fn test_bad_start_uid() {
    let test_key = |key: Vec<u8>| {
        let mut key = split_key(&key, 10).expect("splits");
        assert_eq!(key[0], BYTE_SPLIT);
        assert!(parse(&key).is_ok());
        key.push(0);
        assert!(parse(&key).is_err());
    };

    test_key(data_key(&attr_in_root_namespace("aa"), 1).unwrap());
    test_key(reverse_key(&attr_in_root_namespace("aa"), 1).unwrap());
    test_key(count_key(&attr_in_root_namespace("aa"), 0, false).unwrap());
    test_key(count_key(&attr_in_root_namespace("aa"), 0, true).unwrap());
}

#[test]
fn test_bad_keys() {
    // 0-len key
    assert!(parse(&[]).is_err());

    // key of len < 3
    assert!(parse(&[1]).is_err());
    assert!(parse(&[1, 2]).is_err());

    // key of len < sz (key[1], key[2])
    assert!(parse(&[1, 0x00, 0x04, 1, 2]).is_err());

    // key with uid = 0 is invalid
    let key = data_key(&attr_in_root_namespace("bad uid"), 0).unwrap();
    assert!(parse(&key).is_err());
}

#[test]
fn test_split_key_rejects_non_default_prefix() {
    let err = split_key(&schema_key(&attr_in_root_namespace("aa")).unwrap(), 1)
        .expect_err("a schema key does not split");
    assert_eq!(
        err.to_string(),
        "only keys with default prefix can have a split key"
    );
}

#[test]
fn test_key_rejects_attribute_without_namespace() {
    // Dgraph panics indexing past the attribute's split.
    let err = data_key("no namespace separator", 1).expect_err("no separator");
    assert_eq!(
        err,
        KeyError::MissingNamespaceSeparator("no namespace separator".to_string())
    );
    assert_eq!(
        err.to_string(),
        r#"Attribute "no namespace separator" has no namespace separator "-""#
    );
}

#[test]
fn test_key_rejects_attribute_of_u16_max_bytes() {
    // Dgraph's AssertTrue exits the process.
    let err = schema_key(&attr_in_root_namespace(&"a".repeat(usize::from(u16::MAX))))
        .expect_err("too long");
    assert_eq!(err.0, KeyErrorEnum::AttributeTooLong { len: 65535 });
    assert_eq!(
        err.to_string(),
        "Attribute of 65535 bytes is too long for a key, which holds at most 65534"
    );
}

#[test]
fn test_key_takes_attribute_below_u16_max_bytes() {
    let attr = "a".repeat(usize::from(u16::MAX) - 1);
    let pk = parse(&schema_key(&attr_in_root_namespace(&attr)).unwrap()).expect("parses");
    assert_eq!(parse_attr(pk.attr()).unwrap(), attr);
}
