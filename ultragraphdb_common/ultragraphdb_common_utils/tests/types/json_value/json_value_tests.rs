/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_utils::{JsonValue, go_json};
use ultragraphdb_tool_bdd::{TestError, TestResult};

#[test]
fn objects_keep_their_members_in_order_with_duplicates() -> TestResult {
    let v = go_json::unmarshal(br#"{"b":1,"a":"x","b":[true,null]}"#)?;
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
    Ok(())
}

#[test]
fn numbers_keep_their_text() -> TestResult {
    let v = go_json::unmarshal(b"[-0.0e+00, 1E400, 123456789012345678901234567890]")?;
    assert_eq!(
        v,
        JsonValue::Array(
            ["-0.0e+00", "1E400", "123456789012345678901234567890"]
                .map(|n| JsonValue::Number(n.to_string()))
                .to_vec()
        )
    );
    Ok(())
}

#[test]
fn strings_replace_invalid_utf8_and_lone_surrogates() -> TestResult {
    let v =
        go_json::unmarshal(b"[\"a\xffb\", \"\\ud800\\u0041\", \"\\ud83d\\ude00\", \"\xe2\x82\"]")?;
    assert_eq!(
        v,
        JsonValue::Array(
            ["a\u{fffd}b", "\u{fffd}A", "\u{1f600}", "\u{fffd}\u{fffd}"]
                .map(|s| JsonValue::String(s.to_string()))
                .to_vec()
        )
    );
    Ok(())
}

#[test]
fn the_kind_is_what_go_names_in_a_type_error() -> TestResult {
    for (input, kind) in [
        ("null", "null"),
        ("true", "bool"),
        ("1", "number"),
        (r#""a""#, "string"),
        ("[]", "array"),
        ("{}", "object"),
    ] {
        assert_eq!(go_json::unmarshal(input.as_bytes())?.kind(), kind);
    }
    Ok(())
}

#[test]
fn a_value_ten_thousand_deep_builds_and_drops() -> TestResult {
    let arrays = format!("{}{}", "[".repeat(10000), "]".repeat(10000));
    let array = go_json::unmarshal(arrays.as_bytes())?;
    assert_eq!(array.kind(), "array");
    assert_eq!(array.marshal(), arrays);
    assert_eq!(array.go_v(), arrays);
    let objects = format!(r#"{}1{}"#, r#"{"a":"#.repeat(10000), "}".repeat(10000));
    let object = go_json::unmarshal(objects.as_bytes())?;
    assert_eq!(object.marshal(), objects);
    // Go's %v of the value: map[a:map[a:...1]...].
    let go_v = format!("{}1{}", "map[a:".repeat(10000), "]".repeat(10000));
    assert_eq!(object.go_v(), go_v);
    Ok(())
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "the test clones on a small stack to prove the clone does not recurse"
)]
fn a_clone_is_equal_and_copies_ten_thousand_deep() -> TestResult {
    let small = go_json::unmarshal(br#"{"a":[1,{"b":null,"c":"x"},[]],"d":true,"a":{}}"#)?;
    assert_eq!(small.clone(), small);
    let data = format!(r#"{}"x"{}"#, r#"{"a":[ "#.repeat(5000), "]}".repeat(5000));
    let deep = go_json::unmarshal(data.as_bytes())?;
    let copy = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || deep.clone().marshal())?
        .join()
        .map_err(|_| TestError::new("the cloning thread panicked"))?;
    // json.Marshal writes the value without the space after each bracket.
    assert_eq!(copy, data.replace("[ ", "["));
    Ok(())
}
