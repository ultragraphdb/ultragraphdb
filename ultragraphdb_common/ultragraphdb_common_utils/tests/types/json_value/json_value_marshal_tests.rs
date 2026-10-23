/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Expected values are Go 1.27's `json.Marshal` of the value `json.Decoder.Decode` gives with `UseNumber`.

use ultragraphdb_common_utils::go_json;

#[test]
fn marshal_is_gos_marshal_of_the_decoded_value() {
    for (input, want) in [
        ("null", "null"),
        (" false ", "false"),
        ("1.50", "1.50"),
        (r#""<\u00e9\u2028>""#, "\"\\u003c\u{e9}\\u2028\\u003e\""),
        (r#"[1, "x", null, [true]]"#, r#"[1,"x",null,[true]]"#),
        (
            r#"{"b": 2, "a": {"d": [], "c": {}}, "b": 3}"#,
            r#"{"a":{"c":{},"d":[]},"b":3}"#,
        ),
        (r#"{"a\u0000": "\u0001"}"#, r#"{"a\u0000":"\u0001"}"#),
    ] {
        assert_eq!(
            go_json::unmarshal(input.as_bytes()).unwrap().marshal(),
            want,
            "{input}"
        );
    }
}
