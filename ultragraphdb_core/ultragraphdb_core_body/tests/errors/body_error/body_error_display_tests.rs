/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Each variant's text names its rule and its values.

use ultragraphdb_core_body::BodyError;
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_each_variant_names_its_rule_and_values() -> TestResult {
    let cases = [
        (
            BodyError::body_too_short(3, 16),
            "a body of 3 bytes is shorter than its 16-byte fixed part and section table",
        ),
        (
            BodyError::section_out_of_bounds(1, 24, 4, 26),
            "section 1, 4 bytes at offset 24, runs past the body's 26 bytes",
        ),
        (
            BodyError::section_offset(1, 20, 24),
            "section 1 starts at offset 20, not at 24",
        ),
        (
            BodyError::padding_not_zero(21),
            "padding byte 21 of a body is not zero",
        ),
        (
            BodyError::trailing_bytes(9, 10),
            "a body holds 10 bytes, past the end of its last section at 9",
        ),
        (
            BodyError::section_length(4, 3, 4),
            "section 4, 3 bytes, is not a whole number of 4-byte records",
        ),
        (
            BodyError::count_mismatch("string text", 4, 2),
            "string text counts 4, and the body holds 2",
        ),
        (
            BodyError::not_utf8(3, 1),
            "the text of section 3 is not UTF-8 at offset 1",
        ),
        (
            BodyError::index_out_of_range(0, 1, 2, 4),
            "entry 1 of section 0 is 2, past its bound 4",
        ),
    ];
    for (error, text) in cases {
        assert_eq!(error.to_string(), text);
    }
    Ok(())
}
