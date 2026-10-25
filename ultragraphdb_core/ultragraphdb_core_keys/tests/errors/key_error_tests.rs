/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_core_keys::{
    KeyError, KeyErrorEnum, ParsedKey, attr_in_root_namespace, data_key, parse, parse_namespace,
    schema_key, split_key,
};

#[test]
fn test_kind_and_display() {
    let err = parse(&[0; 3]).expect_err("too short");
    assert_eq!(err.0, KeyErrorEnum::KeyTooShort);
    assert_eq!(err.to_string(), "Key length less than 9");
    assert!(err.source().is_none());

    let err = split_key(&schema_key(&attr_in_root_namespace("a")).unwrap(), 1)
        .expect_err("not a default prefix");
    assert_eq!(err.0, KeyErrorEnum::SplitNonDefaultPrefix);

    let err = parse(&data_key(&attr_in_root_namespace("a"), 0).unwrap()).expect_err("uid 0");
    assert!(matches!(err.0, KeyErrorEnum::ZeroUid { .. }));
    assert_eq!(
        err.to_string(),
        "Invalid UID with value 0 for key: [0 1 97 0 0 0 0 0 0 0 0 0]"
    );
}

/// Every constructor builds its own variant, and every variant displays Dgraph's text: `%v` of the key for the
/// unquoted messages, `%q` and the parsed key's `String` for the quoted ones (hand-derived from `x/keys.go`).
#[test]
fn test_every_constructor_and_display() {
    let key = b"\x00\x02a\"\n".to_vec();
    let parsed = ParsedKey::default();
    let parsed_text = "UID: 0, Attr: , Data key, prefix; 0, byte: 0";
    let num = match parse_namespace("x-a").expect_err("not hexadecimal").0 {
        KeyErrorEnum::InvalidNamespace { error, .. } => error,
        other => panic!("{other:?}"),
    };
    let cases: Vec<(KeyError, KeyErrorEnum, String)> = vec![
        (
            KeyError::KeyTooShort(),
            KeyErrorEnum::KeyTooShort,
            "Key length less than 9".into(),
        ),
        (
            KeyError::InvalidFormat(key.clone()),
            KeyErrorEnum::InvalidFormat { key: key.clone() },
            "Invalid format for key [0 2 97 34 10]".into(),
        ),
        (
            KeyError::InvalidFormat(vec![]),
            KeyErrorEnum::InvalidFormat { key: vec![] },
            "Invalid format for key []".into(),
        ),
        (
            KeyError::InvalidSize(7, key.clone()),
            KeyErrorEnum::InvalidSize {
                size: 7,
                key: key.clone(),
            },
            "Invalid size 7 for key [0 2 97 34 10]".into(),
        ),
        (
            KeyError::AttributeNotUtf8(key.clone()),
            KeyErrorEnum::AttributeNotUtf8 { key: key.clone() },
            "Invalid UTF-8 attribute for key [0 2 97 34 10]".into(),
        ),
        (
            KeyError::UidTooShort(key.clone(), parsed.clone()),
            KeyErrorEnum::UidTooShort {
                key: key.clone(),
                parsed: parsed.clone(),
            },
            format!(r#"uid length < 8 for key: "\x00\x02a\"\n", parsed key: {parsed_text}"#),
        ),
        (
            KeyError::ZeroUid(key.clone()),
            KeyErrorEnum::ZeroUid { key: key.clone() },
            "Invalid UID with value 0 for key: [0 2 97 34 10]".into(),
        ),
        (
            KeyError::StartUidLength(key.clone(), parsed.clone()),
            KeyErrorEnum::StartUidLength {
                key: key.clone(),
                parsed: parsed.clone(),
            },
            format!(r#"StartUid length != 8 for key: "\x00\x02a\"\n", parsed key: {parsed_text}"#),
        ),
        (
            KeyError::StartUidTooShort(key.clone(), parsed.clone()),
            KeyErrorEnum::StartUidTooShort {
                key: key.clone(),
                parsed: parsed.clone(),
            },
            format!(r#"StartUid length < 8 for key: "\x00\x02a\"\n", parsed key: {parsed_text}"#),
        ),
        (
            KeyError::CountTooShort(key.clone(), parsed.clone()),
            KeyErrorEnum::CountTooShort {
                key: key.clone(),
                parsed: parsed.clone(),
            },
            format!(r#"count length < 4 for key: "\x00\x02a\"\n", parsed key: {parsed_text}"#),
        ),
        (
            KeyError::InvalidDataType(),
            KeyErrorEnum::InvalidDataType,
            "Invalid data type".into(),
        ),
        (
            KeyError::SplitNonDefaultPrefix(),
            KeyErrorEnum::SplitNonDefaultPrefix,
            "only keys with default prefix can have a split key".into(),
        ),
        (
            KeyError::MissingNamespaceSeparator("a\"b".into()),
            KeyErrorEnum::MissingNamespaceSeparator {
                attr: "a\"b".into(),
            },
            r#"Attribute "a\"b" has no namespace separator "-""#.into(),
        ),
        (
            KeyError::InvalidNamespace("x-a".into(), num.clone()),
            KeyErrorEnum::InvalidNamespace {
                attr: "x-a".into(),
                error: num.clone(),
            },
            r#"Invalid namespace in attribute "x-a": strconv.ParseUint: parsing "x": invalid syntax"#.into(),
        ),
        (
            KeyError::AttributeTooLong(65535),
            KeyErrorEnum::AttributeTooLong { len: 65535 },
            "Attribute of 65535 bytes is too long for a key, which holds at most 65534".into(),
        ),
        (
            KeyError::TrustMarkerMissing(),
            KeyErrorEnum::TrustMarkerMissing,
            "x.RegisterReservedNamespace: ValueLocked is set but TrustMarker is nil; a value-locked predicate with \
             no TrustMarker is unwritable by everyone, including its owner"
                .into(),
        ),
        (
            KeyError::QualifiedReservedName("0-dgraph.x".into()),
            KeyErrorEnum::QualifiedReservedName {
                name: "0-dgraph.x".into(),
            },
            r#"x.RegisterReservedNamespace: name "0-dgraph.x" must be bare, without the "-" namespace separator"#
                .into(),
        ),
        (
            KeyError::EmptyValueLockedPrefix(),
            KeyErrorEnum::EmptyValueLockedPrefix,
            "x.RegisterReservedNamespace: value-locked prefix must not be empty".into(),
        ),
        (
            KeyError::AlreadyRegistered("type", "dgraph.t".into()),
            KeyErrorEnum::AlreadyRegistered {
                kind: "type",
                name: "dgraph.t".into(),
            },
            r#"x.RegisterReservedNamespace: type "dgraph.t" already registered"#.into(),
        ),
    ];
    for (err, kind, text) in cases {
        assert_eq!(err.0, kind, "{text}");
        assert_eq!(err.to_string(), text);
        assert!(err.source().is_none(), "{text}");
    }
}
