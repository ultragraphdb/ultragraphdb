/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_num::NumError;

// Each expected text is Go 1.26's `(*strconv.NumError).Error()` for the same function and input, and each `num` is
// Go's `strconv.Quote` of that input, both printed by a Go program; the first three inputs are Go's own
// `numErrorTests` (src/strconv/number_test.go).

#[test]
fn test_syntax_display_is_gos_text() {
    let cases = [
        (
            "ParseFloat",
            r#""0""#,
            r#"strconv.ParseFloat: parsing "0": invalid syntax"#,
        ),
        (
            "ParseFloat",
            r#""`""#,
            r#"strconv.ParseFloat: parsing "`": invalid syntax"#,
        ),
        (
            "ParseFloat",
            r#""1\x00.2""#,
            r#"strconv.ParseFloat: parsing "1\x00.2": invalid syntax"#,
        ),
        (
            "Atoi",
            r#""""#,
            r#"strconv.Atoi: parsing "": invalid syntax"#,
        ),
        (
            "ParseBool",
            r#""yes""#,
            r#"strconv.ParseBool: parsing "yes": invalid syntax"#,
        ),
        (
            "ParseInt",
            r#""12a""#,
            r#"strconv.ParseInt: parsing "12a": invalid syntax"#,
        ),
        (
            "ParseFloat",
            r#""\xff""#,
            r#"strconv.ParseFloat: parsing "\xff": invalid syntax"#,
        ),
        (
            "ParseInt",
            "\"\u{263a}\"",
            "strconv.ParseInt: parsing \"\u{263a}\": invalid syntax",
        ),
        (
            "ParseInt",
            r#""a\"b\\c\n""#,
            r#"strconv.ParseInt: parsing "a\"b\\c\n": invalid syntax"#,
        ),
    ];
    for (func, num, want) in cases {
        assert_eq!(
            NumError::Syntax(func, num.to_string()).to_string(),
            want,
            "{func} {num}"
        );
    }
}

#[test]
fn test_range_display_is_gos_text() {
    let cases = [
        (
            "ParseInt",
            r#""9223372036854775808""#,
            r#"strconv.ParseInt: parsing "9223372036854775808": value out of range"#,
        ),
        (
            "ParseInt",
            r#""-9223372036854775809""#,
            r#"strconv.ParseInt: parsing "-9223372036854775809": value out of range"#,
        ),
        (
            "ParseUint",
            r#""18446744073709551616""#,
            r#"strconv.ParseUint: parsing "18446744073709551616": value out of range"#,
        ),
        (
            "ParseFloat",
            r#""1e309""#,
            r#"strconv.ParseFloat: parsing "1e309": value out of range"#,
        ),
        (
            "Atoi",
            r#""99999999999999999999""#,
            r#"strconv.Atoi: parsing "99999999999999999999": value out of range"#,
        ),
    ];
    for (func, num, want) in cases {
        assert_eq!(
            NumError::Range(func, num.to_string()).to_string(),
            want,
            "{func} {num}"
        );
    }
}

#[test]
fn test_base_display_is_gos_text() {
    // Go's ParseUint("10", 1, 64), ParseInt("10", 37, 64) and ParseUint("0", 4294967295, 64).
    let cases = [
        (
            "ParseUint",
            r#""10""#,
            1,
            r#"strconv.ParseUint: parsing "10": invalid base 1"#,
        ),
        (
            "ParseInt",
            r#""10""#,
            37,
            r#"strconv.ParseInt: parsing "10": invalid base 37"#,
        ),
        (
            "ParseUint",
            r#""0""#,
            u32::MAX,
            r#"strconv.ParseUint: parsing "0": invalid base 4294967295"#,
        ),
    ];
    for (func, num, base, want) in cases {
        assert_eq!(
            NumError::Base(func, num.to_string(), base).to_string(),
            want,
            "{func} {num} {base}"
        );
    }
}

#[test]
fn test_display_writes_num_verbatim() {
    // The input is stored already quoted; Display adds no quotes or escapes of its own.
    assert_eq!(
        NumError::Syntax("ParseInt", String::new()).to_string(),
        "strconv.ParseInt: parsing : invalid syntax"
    );
    assert_eq!(
        NumError::Range("ParseInt", "1\n2".to_string()).to_string(),
        "strconv.ParseInt: parsing 1\n2: value out of range"
    );
}
