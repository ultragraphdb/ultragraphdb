/*
 * Copyright 2010 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/encoding/json/v2_decode_test.go (TestUnmarshalTypeErrorMessage). Changed for
 * ultragraphdb: translated to Rust; the texts of the other variants added.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_error_json::JsonError;

#[test]
fn test_unmarshal_type_error_message() {
    let err = |field: &str| {
        JsonError::UnmarshalType(
            "number 5".to_string(),
            "int".to_string(),
            "Root".to_string(),
            field.to_string(),
        )
        .to_string()
    };
    for (field, want) in [
        (
            "",
            "json: cannot unmarshal number 5 into Go struct field Root. of type int",
        ),
        (
            "1",
            "json: cannot unmarshal number 5 into Root.1 of type int",
        ),
        (
            "foo",
            "json: cannot unmarshal number 5 into Go struct field Root.foo of type int",
        ),
        (
            "foo.1",
            "json: cannot unmarshal number 5 into Root.foo.1 of type int",
        ),
        (
            "foo.bar",
            "json: cannot unmarshal number 5 into Go struct field Root.foo.bar of type int",
        ),
        (
            "foo.bar.1",
            "json: cannot unmarshal number 5 into Root.foo.bar.1 of type int",
        ),
        (
            "foo.bar.baz",
            "json: cannot unmarshal number 5 into Go struct field Root.foo.bar.baz of type int",
        ),
    ] {
        assert_eq!(err(field), want, "{field:?}");
    }
}

#[test]
fn a_type_error_at_the_root_names_the_go_value() {
    let e = JsonError::UnmarshalType(
        "string".to_string(),
        "[]interface {}".to_string(),
        String::new(),
        String::new(),
    );
    assert_eq!(
        e.to_string(),
        "json: cannot unmarshal string into Go value of type []interface {}"
    );
    // A path without a root type's name still names the field.
    let e = JsonError::UnmarshalType(
        "number".to_string(),
        "string".to_string(),
        String::new(),
        "type".to_string(),
    );
    assert_eq!(
        e.to_string(),
        "json: cannot unmarshal number into Go struct field .type of type string"
    );
}

#[test]
fn the_other_errors_are_gos_texts() {
    assert_eq!(JsonError::Eof().to_string(), "EOF");
    assert_eq!(JsonError::UnexpectedEof().to_string(), "unexpected EOF");
    assert_eq!(
        JsonError::Syntax(
            "invalid character 'x' looking for beginning of value".to_string(),
            1
        )
        .to_string(),
        "invalid character 'x' looking for beginning of value"
    );
    assert_eq!(
        JsonError::Read("broken pipe".to_string()).to_string(),
        "broken pipe"
    );
}
