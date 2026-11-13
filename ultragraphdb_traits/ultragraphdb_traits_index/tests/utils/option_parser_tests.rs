/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_index::{
    OptionValue, float32_opt_parser, float64_opt_parser, int_opt_parser, string_opt_parser,
    uint_opt_parser,
};

/// The standard library's message for refusing `text` as a `T`.
fn refused<T>(text: &str) -> String
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    text.parse::<T>()
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default()
}

#[test]
fn test_int_opt_parser() -> TestResult {
    assert_eq!(
        int_opt_parser("-9223372036854775808")?,
        OptionValue::Int(i64::MIN)
    );
    assert_eq!(
        int_opt_parser("9223372036854775808")
            .err()
            .context("the error of int_opt_parser(\"9223372036854775808\")")?
            .to_string(),
        refused::<i64>("9223372036854775808")
    );
    assert_eq!(int_opt_parser("+7")?, OptionValue::Int(7));
    assert_eq!(
        int_opt_parser("-")
            .err()
            .context("the error of int_opt_parser(\"-\")")?
            .to_string(),
        refused::<i64>("-")
    );
    Ok(())
}

#[test]
fn test_uint_opt_parser() -> TestResult {
    assert_eq!(
        uint_opt_parser("18446744073709551615")?,
        OptionValue::Uint(u64::MAX)
    );
    assert_eq!(
        uint_opt_parser("-1")
            .err()
            .context("the error of uint_opt_parser(\"-1\")")?
            .to_string(),
        refused::<u64>("-1")
    );
    assert_eq!(
        uint_opt_parser("18446744073709551616")
            .err()
            .context("the error of uint_opt_parser(\"18446744073709551616\")")?
            .to_string(),
        refused::<u64>("18446744073709551616")
    );
    Ok(())
}

#[test]
fn test_float_and_string_opt_parsers() -> TestResult {
    assert_eq!(string_opt_parser("a b")?, OptionValue::Str("a b".into()));
    assert_eq!(float64_opt_parser("-2.5e3")?, OptionValue::Float(-2500.0));
    assert_eq!(
        float64_opt_parser("x")
            .err()
            .context("the error of float64_opt_parser(\"x\")")?
            .to_string(),
        refused::<f64>("x")
    );
    assert_eq!(
        float64_opt_parser("1e999")
            .err()
            .context("the error of float64_opt_parser(\"1e999\")")?
            .to_string(),
        "number too large to fit in target type"
    );
    assert!(matches!(float64_opt_parser("Inf")?, OptionValue::Float(f) if f.is_infinite()));
    assert_eq!(
        float32_opt_parser("0.1")?,
        OptionValue::Float(f64::from(0.1f32))
    );
    assert_eq!(
        float32_opt_parser("1e39")
            .err()
            .context("the error of float32_opt_parser(\"1e39\")")?
            .to_string(),
        "number too large to fit in target type"
    );
    Ok(())
}
