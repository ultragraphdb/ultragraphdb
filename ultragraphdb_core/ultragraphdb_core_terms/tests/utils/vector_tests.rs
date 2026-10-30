/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::{TermError, TermErrorEnum, format_vector, parse_vector};

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
fn test_a_canonical_form_parses_to_its_elements() {
    // Each element a decimal exactly representable in binary32, or the nearest binary32 to it.
    assert_eq!(parse_vector("[0.5, -1, 2.25]"), Ok(vec![0.5, -1.0, 2.25]));
    assert_eq!(
        parse_vector("[0.1, 100000000000000000000, 0.000001]"),
        Ok(vec![0.1, 1e20, 1e-6])
    );
    // -0 keeps its sign bit, 0x80000000, which `==` alone does not tell from +0.
    let zeros: Vec<u32> = ["[0]", "[-0]"]
        .iter()
        .map(|l| parse_vector(l).unwrap()[0].to_bits())
        .collect();
    assert_eq!(zeros, vec![0x0000_0000, 0x8000_0000]);
    // The largest finite binary32, (2 - 2^-23) × 2^127, and the least subnormal, 2^-149, each written in full.
    assert_eq!(
        parse_vector("[340282350000000000000000000000000000000]"),
        Ok(vec![f32::MAX])
    );
    let least = format!("[0.{}1]", "0".repeat(44));
    assert_eq!(parse_vector(&least), Ok(vec![f32::from_bits(1)]));
}

#[test]
fn test_elements_format_as_the_shortest_decimal_without_exponent() {
    // PROTOCOL.md section 10.2's vector form, written by hand.
    assert_eq!(
        format_vector([0.5f32, -1.0, 2.25].into_iter()),
        "[0.5, -1, 2.25]"
    );
    assert_eq!(
        format_vector([1e20f32, -0.0, 1e-6].into_iter()),
        "[100000000000000000000, -0, 0.000001]"
    );
    assert_eq!(format_vector([7.0f32].into_iter()), "[7]");
}

#[test]
fn test_any_other_form_is_refused_with_its_reason() {
    let cases = [
        ("0.5", "not in brackets"),
        ("", "not in brackets"),
        ("[1", "not in brackets"),
        ("1]", "not in brackets"),
        ("[1]x", "not in brackets"),
        ("[]", "empty"),
        ("[0.50]", "not in canonical form"),
        ("[-0.0]", "not in canonical form"),
        ("[+1]", "not in canonical form"),
        ("[0.5,1]", "an element is no 32-bit float"),
        ("[1, ]", "an element is no 32-bit float"),
        ("[ 1]", "an element is no 32-bit float"),
        ("[1,  2]", "an element is no 32-bit float"),
        ("[1e3]", "not in canonical form"),
        ("[inf]", "an element is not finite"),
        ("[-inf]", "an element is not finite"),
        ("[NaN]", "an element is not finite"),
        // Past the largest finite binary32, which a decimal rounds to infinity.
        (
            "[340282370000000000000000000000000000000]",
            "an element is not finite",
        ),
        ("[x]", "an element is no 32-bit float"),
    ];
    for (lexical, reason) in cases {
        assert_eq!(
            parse_vector(lexical),
            Err(TermError::InvalidVector(lexical.to_string(), reason)),
            "{lexical:?}"
        );
    }
}

#[test]
fn test_a_vector_holds_at_most_65535_elements() {
    // PROTOCOL.md section 10.1: the dimension is a u16, 1 to 65,535.
    let too_long = format!("[{}]", vec!["1"; 65_536].join(", "));
    let error = parse_vector(&too_long).unwrap_err();
    let TermErrorEnum::InvalidVector { lexical, reason } = &error.0 else {
        panic!("{error}");
    };
    assert_eq!(
        (lexical == &too_long, *reason),
        (true, "more than 65,535 elements")
    );
    let longest = format!("[{}]", vec!["1"; 65_535].join(", "));
    assert_eq!(parse_vector(&longest).unwrap().len(), 65_535);
}
