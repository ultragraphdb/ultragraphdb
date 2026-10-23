/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_facets::{FacetsErrorEnum, facet_for, sort_and_validate};
use ultragraphdb_proto_dgraph::api;

#[test]
fn each_failure_has_its_kind_and_dgraph_text() {
    let cases: [(&str, FacetsErrorEnum, &str); 6] = [
        (
            "abc",
            FacetsErrorEnum::Unparsable("abc".to_string()),
            "Could not parse the facet value : [abc]",
        ),
        ("NaN", FacetsErrorEnum::NaN, "Got invalid value: NaN"),
        (
            "99999999999999999999",
            FacetsErrorEnum::Number(
                r#"strconv.ParseInt: parsing "99999999999999999999": value out of range"#
                    .to_string(),
            ),
            r#"strconv.ParseInt: parsing "99999999999999999999": value out of range"#,
        ),
        (
            "0x1p1024",
            FacetsErrorEnum::Number(
                r#"strconv.ParseFloat: parsing "0x1p1024": value out of range"#.to_string(),
            ),
            r#"strconv.ParseFloat: parsing "0x1p1024": value out of range"#,
        ),
        (
            "\"a\u{85}\\q\"",
            FacetsErrorEnum::Unquote("\"a\u{85}\\q\"".to_string()),
            r#"could not unquote "\"a\u0085\\q\"":: invalid syntax"#,
        ),
        (
            r#""\xff""#,
            FacetsErrorEnum::InvalidUtf8("k".to_string()),
            "Facet value of k is not valid UTF-8",
        ),
    ];
    for (input, kind, text) in cases {
        let err = facet_for("k", input).unwrap_err();
        assert_eq!(err.0, kind, "{input}");
        assert_eq!(err.to_string(), text, "{input}");
    }
}

#[test]
fn a_repeated_key_names_the_key() {
    let mut fs = vec![
        api::Facet {
            key: "b".into(),
            ..Default::default()
        },
        api::Facet {
            key: "b".into(),
            ..Default::default()
        },
    ];
    let err = sort_and_validate(&mut fs).unwrap_err();
    assert_eq!(err.0, FacetsErrorEnum::RepeatedKey("b".to_string()));
    assert_eq!(
        err.to_string(),
        "Repeated keys are not allowed in facets. But got b"
    );
}

#[test]
fn the_error_is_a_std_error() {
    let err: Box<dyn std::error::Error> = Box::new(facet_for("k", "abc").unwrap_err());
    assert_eq!(err.to_string(), "Could not parse the facet value : [abc]");
}
