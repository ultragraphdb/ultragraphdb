/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/keys_test.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::panic;

use ultragraphdb_core_keys::{
    NS_SEPARATOR, ROOT_NAMESPACE, attr_in_root_namespace, data_key, is_reverse_attr,
    namespace_attr, namespace_attr_list, namespace_to_bytes, parse, parse_attr, parse_attr_list,
    parse_namespace, parse_namespace_attr, parse_namespace_bytes,
};

use crate::golden::{NamespaceAttrCase, compare, golden};

#[test]
fn test_name_space() {
    let ns = 133u64;
    let attr = "name";
    let ns_attr = namespace_attr(ns, attr);
    let (parsed_ns, parsed_attr) = parse_namespace_attr(&ns_attr);
    assert_eq!(ns, parsed_ns);
    assert_eq!(attr, parsed_attr);
}

#[test]
fn test_json_marshal() {
    let p = serde_json::json!({ "predicate": namespace_attr(129, "name") });
    let b = serde_json::to_string(&p).expect("marshals");

    let p2: serde_json::Value = serde_json::from_str(&b).expect("unmarshals");
    let (ns, attr) = parse_namespace_attr(p2["predicate"].as_str().expect("a string"));
    assert_eq!(129, ns);
    assert_eq!("name", attr);
}

#[test]
fn test_ns_separator() {
    let uid = 10u64;
    let pred = format!("name{NS_SEPARATOR}surname");
    let key = data_key(&attr_in_root_namespace(&pred), uid);
    let pk = parse(&key).expect("parses");
    assert_eq!(uid, pk.uid());
    let (ns, attr) = parse_namespace_attr(pk.attr());
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
            parse_namespace_attr(&formatted),
            (c.parsed_namespace, c.parsed_attr.as_str())
        );
        assert_eq!(parse_namespace(&formatted), c.parsed_namespace);
        assert_eq!(parse_attr(&formatted), c.parsed_attr);
        let (ns_bytes, attr) = parse_namespace_bytes(&formatted);
        compare(&formatted, &c.namespace_bytes, &ns_bytes);
        compare(
            &formatted,
            &c.namespace_bytes,
            &namespace_to_bytes(c.namespace),
        );
        assert_eq!(attr, c.parsed_attr);
        if c.is_reverse_attr_panics {
            assert!(
                panic::catch_unwind(|| is_reverse_attr(&formatted)).is_err(),
                "{formatted}"
            );
        } else {
            assert_eq!(
                is_reverse_attr(&formatted),
                c.is_reverse_attr,
                "{formatted}"
            );
        }
    }
}

#[test]
fn test_namespace_attr_lists() {
    let preds = vec!["name".to_string(), "age".to_string()];
    let attrs = namespace_attr_list(0x2a, &preds);
    assert_eq!(attrs, vec!["2a-name", "2a-age"]);
    assert_eq!(parse_attr_list(&attrs), preds);
    assert!(namespace_attr_list(1, &[]).is_empty());
}

#[test]
#[should_panic]
fn test_parse_namespace_attr_without_separator_panics() {
    parse_namespace_attr("name");
}

#[test]
#[should_panic]
fn test_parse_namespace_attr_with_non_hex_namespace_panics() {
    parse_namespace_attr("xyz-name");
}
