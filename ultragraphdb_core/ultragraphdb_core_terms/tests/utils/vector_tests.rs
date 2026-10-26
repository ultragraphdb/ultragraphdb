/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::{TermErrorEnum, format_vector, parse_vector};

#[test]
fn test_the_canonical_form_round_trips() {
    for lexical in [
        "[0.5, -1, 2.25]",
        "[0]",
        "[-0]",
        "[0.1, 100000000000000000000, 0.000001]",
    ] {
        let elements = parse_vector(lexical).unwrap();
        assert_eq!(format_vector(elements.into_iter()), lexical);
    }
}

#[test]
fn test_any_other_form_is_refused_with_its_reason() {
    let cases = [
        ("0.5", "not in brackets"),
        ("[]", "empty"),
        ("[0.50]", "not in canonical form"),
        ("[0.5,1]", "an element is no 32-bit float"),
        ("[1e3]", "not in canonical form"),
        ("[inf]", "an element is not finite"),
        ("[NaN]", "an element is not finite"),
        ("[x]", "an element is no 32-bit float"),
    ];
    for (lexical, reason) in cases {
        let error = parse_vector(lexical).unwrap_err();
        assert!(
            matches!(&error.0, TermErrorEnum::InvalidVector { reason: r, .. } if *r == reason),
            "{lexical}: {error}"
        );
    }
    let too_long = format!("[{}]", vec!["1"; 65_536].join(", "));
    assert!(parse_vector(&too_long).is_err());
    let longest = format!("[{}]", vec!["1"; 65_535].join(", "));
    assert_eq!(parse_vector(&longest).unwrap().len(), 65_535);
}
