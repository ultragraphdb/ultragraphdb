/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Values: their kind told by their first characters, a feature outside the subset refused by name.

use crate::errors::toml_error::TomlError;
use crate::types::toml_feature::TomlFeature;
use crate::types::toml_invalid::TomlInvalid;
use crate::types::toml_value::TomlValue;
use crate::utils::chars::split_run;
use crate::utils::string::basic_string;

/// The value at the start of `rest`, after a key's `=` and its whitespace, and the rest of the line after it.
///
/// # Errors
///
/// `Unsupported` for a value outside the subset, `Invalid` for one TOML refuses, both on line `line`.
pub fn value(rest: &str, line: usize) -> Result<(TomlValue, &str), TomlError> {
    let unsupported = |feature| Err(TomlError::unsupported(feature, line));
    let mut chars = rest.chars();
    match chars.next() {
        None | Some('#') => Err(TomlError::invalid(TomlInvalid::KeyWithoutValue, line)),
        Some('"') if rest.starts_with("\"\"\"") => unsupported(TomlFeature::MultiLineStrings),
        Some('"') => {
            let (text, after) = basic_string(chars.as_str(), line)?;
            Ok((TomlValue::String(text), after))
        }
        Some('\'') if rest.starts_with("'''") => unsupported(TomlFeature::MultiLineStrings),
        Some('\'') => unsupported(TomlFeature::LiteralStrings),
        Some('[') => unsupported(TomlFeature::Arrays),
        Some('{') => unsupported(TomlFeature::InlineTables),
        Some(_) => {
            let (token, after) = split_run(rest, is_token);
            Ok((scalar(token, line)?, after))
        }
    }
}

/// A character of a bare value: a boolean, a number, a date or a time.
fn is_token(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-' | '.' | ':')
}

/// The boolean or integer `token` spells.
fn scalar(token: &str, line: usize) -> Result<TomlValue, TomlError> {
    if token == "true" {
        return Ok(TomlValue::Boolean(true));
    }
    if token == "false" {
        return Ok(TomlValue::Boolean(false));
    }
    let unsigned = token.strip_prefix(['+', '-']).unwrap_or(token);
    if unsigned == "inf" || unsigned == "nan" {
        return Err(TomlError::unsupported(TomlFeature::InfinitiesAndNan, line));
    }
    match token.chars().next() {
        Some(c) if c.is_ascii_digit() || matches!(c, '+' | '-' | '_') => {
            number(token, unsigned, line).map(TomlValue::Integer)
        }
        Some(_) | None => Err(TomlError::invalid(TomlInvalid::MalformedValue, line)),
    }
}

/// The decimal integer `token`, with `unsigned` its digits after any sign; a token that is another kind of number
/// is refused by its feature.
fn number(token: &str, unsigned: &str, line: usize) -> Result<i64, TomlError> {
    let feature = if unsigned.starts_with("0x") {
        Some(TomlFeature::HexadecimalIntegers)
    } else if unsigned.starts_with("0o") {
        Some(TomlFeature::OctalIntegers)
    } else if unsigned.starts_with("0b") {
        Some(TomlFeature::BinaryIntegers)
    } else if is_date_or_time(unsigned) {
        Some(TomlFeature::DatesAndTimes)
    } else if unsigned.contains(['.', 'e', 'E']) {
        Some(TomlFeature::Floats)
    } else {
        None
    };
    match feature {
        Some(feature) => Err(TomlError::unsupported(feature, line)),
        None => integer(token, line),
    }
}

/// A date starts with four digits and `-`, a time with two digits and `:` (TOML 1.0's `full-date`, `partial-time`).
fn is_date_or_time(unsigned: &str) -> bool {
    match unsigned.as_bytes() {
        [a, b, c, d, b'-', ..] => [a, b, c, d].iter().all(|x| x.is_ascii_digit()),
        [a, b, b':', ..] => a.is_ascii_digit() && b.is_ascii_digit(),
        _ => false,
    }
}

/// The decimal integer `token`: an optional sign, then digits with `_` only between two of them, no leading zero,
/// within an `i64`.
fn integer(token: &str, line: usize) -> Result<i64, TomlError> {
    let invalid = |reason| TomlError::invalid(reason, line);
    let (negative, digits) = match token.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, token.strip_prefix('+').unwrap_or(token)),
    };
    if digits.is_empty()
        || digits.starts_with('_')
        || digits.ends_with('_')
        || digits.contains("__")
        || !digits.chars().all(|c| c.is_ascii_digit() || c == '_')
    {
        return Err(invalid(TomlInvalid::MalformedInteger));
    }
    if digits.len() > 1 && digits.starts_with('0') {
        return Err(invalid(TomlInvalid::LeadingZero));
    }
    // Accumulated below zero, where i64::MIN fits and i64::MAX's negation does too.
    let mut value: i64 = 0;
    for digit in digits.chars().filter_map(|c| c.to_digit(10)) {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_sub(i64::from(digit)))
            .ok_or_else(|| invalid(TomlInvalid::IntegerOutOfRange))?;
    }
    if negative {
        Ok(value)
    } else {
        value
            .checked_neg()
            .ok_or_else(|| invalid(TomlInvalid::IntegerOutOfRange))
    }
}
