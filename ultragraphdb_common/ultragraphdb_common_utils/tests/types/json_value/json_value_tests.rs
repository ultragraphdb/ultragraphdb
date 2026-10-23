/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_utils::{JsonValue, go_json};

#[test]
fn objects_keep_their_members_in_order_with_duplicates() {
    let v = go_json::unmarshal(br#"{"b":1,"a":"x","b":[true,null]}"#).unwrap();
    assert_eq!(
        v,
        JsonValue::Object(vec![
            ("b".to_string(), JsonValue::Number("1".to_string())),
            ("a".to_string(), JsonValue::String("x".to_string())),
            (
                "b".to_string(),
                JsonValue::Array(vec![JsonValue::Bool(true), JsonValue::Null])
            ),
        ])
    );
}

#[test]
fn numbers_keep_their_text() {
    let v = go_json::unmarshal(b"[-0.0e+00, 1E400, 123456789012345678901234567890]").unwrap();
    assert_eq!(
        v,
        JsonValue::Array(
            ["-0.0e+00", "1E400", "123456789012345678901234567890"]
                .map(|n| JsonValue::Number(n.to_string()))
                .to_vec()
        )
    );
}

#[test]
fn strings_replace_invalid_utf8_and_lone_surrogates() {
    let v =
        go_json::unmarshal(b"[\"a\xffb\", \"\\ud800\\u0041\", \"\\ud83d\\ude00\", \"\xe2\x82\"]")
            .unwrap();
    assert_eq!(
        v,
        JsonValue::Array(
            ["a\u{fffd}b", "\u{fffd}A", "\u{1f600}", "\u{fffd}\u{fffd}"]
                .map(|s| JsonValue::String(s.to_string()))
                .to_vec()
        )
    );
}

#[test]
fn the_kind_is_what_go_names_in_a_type_error() {
    for (input, kind) in [
        ("null", "null"),
        ("true", "bool"),
        ("1", "number"),
        (r#""a""#, "string"),
        ("[]", "array"),
        ("{}", "object"),
    ] {
        assert_eq!(go_json::unmarshal(input.as_bytes()).unwrap().kind(), kind);
    }
}

#[test]
fn a_value_ten_thousand_deep_builds_and_drops() {
    let data = format!("{}{}", "[".repeat(10000), "]".repeat(10000));
    let v = go_json::unmarshal(data.as_bytes()).unwrap();
    assert_eq!(v.kind(), "array");
    let data = format!(r#"{}1{}"#, r#"{"a":"#.repeat(10000), "}".repeat(10000));
    let v = go_json::unmarshal(data.as_bytes()).unwrap();
    assert_eq!(v.marshal().len(), data.len());
    assert_eq!(v.go_v().len(), "map[a:".len() * 10000 + 1 + 10000);
}

#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "the test clones on a small stack to prove the clone does not recurse"
)]
fn a_clone_is_equal_and_copies_ten_thousand_deep() {
    let v = go_json::unmarshal(br#"{"a":[1,{"b":null,"c":"x"},[]],"d":true,"a":{}}"#).unwrap();
    assert_eq!(v.clone(), v);
    let data = format!(r#"{}"x"{}"#, r#"{"a":[ "#.repeat(5000), "]}".repeat(5000));
    let v = go_json::unmarshal(data.as_bytes()).unwrap();
    let copy = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || v.clone().marshal())
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(copy.len(), data.len() - 5000);
}
