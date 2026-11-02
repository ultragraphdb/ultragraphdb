/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Values: basic strings with every escape, decimal integers at and past the bounds of an `i64`, and booleans.

use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_toml::{TomlDocument, TomlValue};
use ultragraphdb_tool_toml::{TomlErrorEnum, TomlInvalid};

use crate::utils::refusal::refusal;

/// The one value of the document `v = <text>`.
fn value(text: &str) -> TestResult<TomlValue> {
    let document = TomlDocument::parse(format!("v = {text}\n").as_bytes())?;
    let table = document.tables().first().context("the root table")?;
    Ok(table
        .entries()
        .first()
        .context("one entry")?
        .value()
        .clone())
}

/// Why `v = <text>` was refused, on line 1.
fn refused(text: &str) -> TestResult<TomlErrorEnum> {
    refusal(format!("v = {text}\n").as_bytes())
}

/// An invalid-document refusal of line 1 for `reason`.
fn invalid(reason: TomlInvalid) -> TomlErrorEnum {
    TomlErrorEnum::Invalid { reason, line: 1 }
}

fn string(text: &str) -> TomlValue {
    TomlValue::String(text.to_string())
}

#[test]
fn test_every_escape_resolves_to_its_character() -> TestResult {
    // TOML 1.0, "String": \b U+0008, \t U+0009, \n U+000A, \f U+000C, \r U+000D, \" U+0022, \\ U+005C.
    let cases = [
        (r#""\b""#, "\u{8}"),
        (r#""\t""#, "\u{9}"),
        (r#""\n""#, "\u{a}"),
        (r#""\f""#, "\u{c}"),
        (r#""\r""#, "\u{d}"),
        (r#""\"""#, "\""),
        (r#""\\""#, "\\"),
        (r#""é""#, "é"),
        (r#""é""#, "é"),
        (r#""\U0001F600""#, "\u{1F600}"),
        (r#""\u0000""#, "\u{0}"),
        (r#""퟿""#, "\u{D7FF}\u{E000}"),
        (r#""\U0010FFFF""#, "\u{10FFFF}"),
        (r#""a\\b\"c\td""#, "a\\b\"c\td"),
    ];
    for (text, expected) in cases {
        assert_eq!(value(text)?, string(expected), "{text}");
    }
    Ok(())
}

#[test]
fn test_strings_keep_what_needs_no_escape() -> TestResult {
    assert_eq!(value(r#""""#)?, string(""));
    assert_eq!(value("\"tab\there\"")?, string("tab\there"));
    assert_eq!(
        value(r#""it's # [x] {y} = 'z'""#)?,
        string("it's # [x] {y} = 'z'")
    );
    assert_eq!(value("\"é ∑ 😀\"")?, string("é ∑ 😀"));
    Ok(())
}

#[test]
fn test_an_undefined_escape_is_invalid() -> TestResult {
    // \a, \x (TOML 1.1), \e (TOML 1.1), \ before a space, a backslash ending the line, and short hex digits.
    for text in [
        r#""\a""#,
        r#""\x41""#,
        r#""\e""#,
        r#""\ ""#,
        r#""\u00G9""#,
        r#""\u00e""#,
        r#""\U0001F60""#,
        "\"\\",
    ] {
        assert_eq!(
            refused(text)?,
            invalid(TomlInvalid::InvalidEscape),
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn test_an_escape_naming_a_surrogate_or_past_the_last_code_point_is_invalid() -> TestResult {
    // U+D800..U+DFFF are surrogates, never scalar values; U+10FFFF is the last code point.
    for text in [
        r#""\uD800""#,
        r#""\uDFFF""#,
        r#""\U0000D800""#,
        r#""\U00110000""#,
        r#""\UFFFFFFFF""#,
    ] {
        assert_eq!(
            refused(text)?,
            invalid(TomlInvalid::InvalidEscape),
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn test_integers_are_decimal_with_a_sign_and_underscores_between_digits() -> TestResult {
    let cases = [
        ("0", 0),
        ("+0", 0),
        ("-0", 0),
        ("7", 7),
        ("+7", 7),
        ("-1_000", -1_000),
        ("1_2_3", 123),
        ("5_349_221", 5_349_221),
        ("10", 10),
    ];
    for (text, expected) in cases {
        assert_eq!(value(text)?, TomlValue::Integer(expected), "{text}");
    }
    Ok(())
}

#[test]
fn test_the_bounds_of_an_i64_parse_and_one_past_each_is_out_of_range() -> TestResult {
    // i64::MAX = 2^63 - 1 = 9223372036854775807, i64::MIN = -2^63 = -9223372036854775808.
    assert_eq!(value("9223372036854775807")?, TomlValue::Integer(i64::MAX));
    assert_eq!(
        value("+9_223_372_036_854_775_807")?,
        TomlValue::Integer(i64::MAX)
    );
    assert_eq!(value("-9223372036854775808")?, TomlValue::Integer(i64::MIN));
    for text in [
        "9223372036854775808",
        "-9223372036854775809",
        "+9223372036854775808",
        "99999999999999999999",
        "-99999999999999999999",
    ] {
        assert_eq!(
            refused(text)?,
            invalid(TomlInvalid::IntegerOutOfRange),
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn test_booleans_are_true_and_false() -> TestResult {
    assert_eq!(value("true")?, TomlValue::Boolean(true));
    assert_eq!(value("false")?, TomlValue::Boolean(false));
    assert_eq!(value("true # yes")?, TomlValue::Boolean(true));
    Ok(())
}
