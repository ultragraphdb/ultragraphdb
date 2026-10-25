/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/keys_test.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_keys::{
    KeyError, KeyErrorEnum, NS_SEPARATOR, ROOT_NAMESPACE, attr_in_root_namespace, data_key,
    is_reverse_attr, namespace_attr, namespace_attr_list, namespace_to_bytes, parse, parse_attr,
    parse_attr_list, parse_namespace, parse_namespace_attr, parse_namespace_bytes,
};

use crate::golden::{NamespaceAttrCase, compare, golden};

#[test]
fn test_name_space() {
    let ns = 133u64;
    let attr = "name";
    let ns_attr = namespace_attr(ns, attr);
    let (parsed_ns, parsed_attr) = parse_namespace_attr(&ns_attr).unwrap();
    assert_eq!(ns, parsed_ns);
    assert_eq!(attr, parsed_attr);
}

#[test]
fn test_json_marshal() {
    let p = serde_json::json!({ "predicate": namespace_attr(129, "name") });
    let b = serde_json::to_string(&p).expect("marshals");

    let p2: serde_json::Value = serde_json::from_str(&b).expect("unmarshals");
    let (ns, attr) = parse_namespace_attr(p2["predicate"].as_str().expect("a string")).unwrap();
    assert_eq!(129, ns);
    assert_eq!("name", attr);
}

#[test]
fn test_ns_separator() {
    let uid = 10u64;
    let pred = format!("name{NS_SEPARATOR}surname");
    let key = data_key(&attr_in_root_namespace(&pred), uid).unwrap();
    let pk = parse(&key).expect("parses");
    assert_eq!(uid, pk.uid());
    let (ns, attr) = parse_namespace_attr(pk.attr()).unwrap();
    assert_eq!(ROOT_NAMESPACE, ns);
    assert_eq!(pred, attr);
}

#[test]
fn test_golden_namespace_attr() {
    let golden = golden::<NamespaceAttrCase>("keys_namespace_attr");
    for c in golden.cases() {
        let formatted = namespace_attr(c.namespace, &c.attr);
        assert_eq!(formatted, c.formatted);
        assert_eq!(
            parse_namespace_attr(&formatted).unwrap(),
            (c.parsed_namespace, c.parsed_attr.as_str())
        );
        assert_eq!(parse_namespace(&formatted).unwrap(), c.parsed_namespace);
        assert_eq!(parse_attr(&formatted).unwrap(), c.parsed_attr);
        let (ns_bytes, attr) = parse_namespace_bytes(&formatted).unwrap();
        compare(&formatted, &c.namespace_bytes, &ns_bytes);
        compare(
            &formatted,
            &c.namespace_bytes,
            &namespace_to_bytes(c.namespace),
        );
        assert_eq!(attr, c.parsed_attr);
        // Go panics reading the first byte of an empty predicate; the port's empty predicate is not a reverse one.
        assert_eq!(
            c.is_reverse_attr_panics,
            c.parsed_attr.is_empty(),
            "{formatted}: Dgraph panics exactly on the empty predicate"
        );
        assert_eq!(
            is_reverse_attr(&formatted).unwrap(),
            c.is_reverse_attr,
            "{formatted}"
        );
    }
}

#[test]
fn test_namespace_attr_lists() {
    let preds = vec!["name".to_string(), "age".to_string()];
    let attrs = namespace_attr_list(0x2a, &preds);
    assert_eq!(attrs, vec!["2a-name", "2a-age"]);
    assert_eq!(parse_attr_list(&attrs).unwrap(), preds);
    assert!(namespace_attr_list(1, &[]).is_empty());
}

#[test]
fn test_parse_namespace_attr_without_separator_is_refused() {
    // Dgraph panics indexing past the split.
    let err = parse_namespace_attr("name").expect_err("no separator");
    assert_eq!(
        err.0,
        KeyErrorEnum::MissingNamespaceSeparator {
            attr: "name".to_string()
        }
    );
    assert_eq!(parse_attr("name"), Err(err));
    assert_eq!(parse_namespace("name").map_err(|e| e.to_string()), Err(
        r#"Invalid namespace in attribute "name": strconv.ParseUint: parsing "name": invalid syntax"#.to_string()
    ));
    assert_eq!(parse_namespace("5"), Ok(5));
}

#[test]
fn test_parse_namespace_attr_with_non_hex_namespace_is_refused() {
    // Dgraph's strToUint exits the process with Go's strconv error.
    let err = parse_namespace_attr("xyz-name").expect_err("not hexadecimal");
    assert!(matches!(err.0, KeyErrorEnum::InvalidNamespace { .. }));
    assert_eq!(
        err.to_string(),
        r#"Invalid namespace in attribute "xyz-name": strconv.ParseUint: parsing "xyz": invalid syntax"#
    );
    // The list stops at the first attribute it refuses.
    assert_eq!(
        parse_attr_list(&["0-a".to_string(), "b".to_string(), "c".to_string()]),
        Err(KeyError::MissingNamespaceSeparator("b".to_string()))
    );
    assert_eq!(
        is_reverse_attr("~a"),
        Err(KeyError::MissingNamespaceSeparator("~a".to_string()))
    );
    assert_eq!(
        parse_namespace_bytes("xyz-name").map_err(|e| e.to_string()),
        Err(err.to_string())
    );
}

#[test]
fn test_parse_namespace_edge_cases() {
    // (attribute, Go's strconv.ParseUint(namespace, 16, 64) result), probed with Go 1.27.
    let cases: [(&str, Result<u64, &str>); 9] = [
        ("FF-a", Ok(255)),
        ("ffffffffffffffff-a", Ok(u64::MAX)),
        ("00000000000000000001-a", Ok(1)),
        (
            "0x1-a",
            Err(r#"strconv.ParseUint: parsing "0x1": invalid syntax"#),
        ),
        (
            "-a",
            Err(r#"strconv.ParseUint: parsing "": invalid syntax"#),
        ),
        (
            "+1-a",
            Err(r#"strconv.ParseUint: parsing "+1": invalid syntax"#),
        ),
        (
            " 1-a",
            Err(r#"strconv.ParseUint: parsing " 1": invalid syntax"#),
        ),
        (
            "1_0-a",
            Err(r#"strconv.ParseUint: parsing "1_0": invalid syntax"#),
        ),
        (
            "10000000000000000-a",
            Err(r#"strconv.ParseUint: parsing "10000000000000000": value out of range"#),
        ),
    ];
    for (attr, expected) in cases {
        let expected =
            expected.map_err(|e| format!("Invalid namespace in attribute {attr:?}: {e}"));
        assert_eq!(
            parse_namespace(attr).map_err(|e| e.to_string()),
            expected,
            "{attr}"
        );
        assert_eq!(
            parse_namespace_attr(attr).map_err(|e| e.to_string()),
            expected.clone().map(|ns| (ns, "a")),
            "{attr}"
        );
        // The predicate alone never reads the namespace.
        assert_eq!(parse_attr(attr), Ok("a"), "{attr}");
        if let Err(e) = parse_namespace(attr) {
            assert!(
                matches!(&e.0, KeyErrorEnum::InvalidNamespace { attr: a, .. } if a == attr),
                "{attr}: {e:?}"
            );
        }
    }
    assert_eq!(namespace_attr(u64::MAX, "a"), "ffffffffffffffff-a");
    assert_eq!(namespace_to_bytes(u64::MAX), [0xff; 8]);
    assert_eq!(
        namespace_to_bytes(0x0102_0304_0506_0708),
        [1, 2, 3, 4, 5, 6, 7, 8]
    );
}
