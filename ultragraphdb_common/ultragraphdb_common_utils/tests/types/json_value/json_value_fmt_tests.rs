/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Expected values are Go 1.27's `fmt.Sprint` of the value `json.Decoder.Decode` gives with `UseNumber`.

use ultragraphdb_common_utils::go_json;

#[test]
fn go_v_is_fmts_v_of_the_decoded_value() {
    for (input, want) in [
        ("null", "<nil>"),
        ("true", "true"),
        ("1.50", "1.50"),
        (r#""a b""#, "a b"),
        (r#"[1, "x", null, [true]]"#, "[1 x <nil> [true]]"),
        (
            r#"{"b": 2, "a": {"d": [], "c": {}}, "b": 3}"#,
            "map[a:map[c:map[] d:[]] b:3]",
        ),
        ("[]", "[]"),
        ("{}", "map[]"),
    ] {
        assert_eq!(
            go_json::unmarshal(input.as_bytes()).unwrap().go_v(),
            want,
            "{input}"
        );
    }
}
