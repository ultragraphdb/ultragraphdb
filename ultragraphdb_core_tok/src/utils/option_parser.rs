/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/options/option_parser.go. Changed for ultragraphdb: translated to Rust; Go's
 * strconv parsers and their error texts.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::errors::options_error::{OptionsError, OptionsErrorEnum};
use crate::types::option_value::OptionValue;

fn syntax(func: &str, s: &str) -> OptionsError {
    OptionsError::new(OptionsErrorEnum::Parse(format!(
        "strconv.{func}: parsing {}: invalid syntax",
        go_quote(s)
    )))
}

fn range(func: &str, s: &str) -> OptionsError {
    OptionsError::new(OptionsErrorEnum::Parse(format!(
        "strconv.{func}: parsing {}: value out of range",
        go_quote(s)
    )))
}

/// Go's `strconv.Quote` for an error message: printable ASCII as is, `"` and `\` escaped, other bytes escaped.
fn go_quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32))
            }
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The decimal digits of Go's `ParseInt`/`ParseUint` with base 10: an optional sign (when `signed`), then digits
/// only.
fn decimal(s: &str, signed: bool) -> Option<(bool, &str)> {
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'+') if signed => (false, &s[1..]),
        Some(b'-') if signed => (true, &s[1..]),
        _ => (false, s),
    };
    (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())).then_some((neg, digits))
}

/// Dgraph's `IntOptParser`: Go's `strconv.Atoi`.
pub fn int_opt_parser(v: &str) -> Result<OptionValue, OptionsError> {
    let Some((neg, digits)) = decimal(v, true) else {
        return Err(syntax("Atoi", v));
    };
    let m: u128 = digits.parse().map_err(|_| range("Atoi", v))?;
    let i = if neg { -(m as i128) } else { m as i128 };
    i64::try_from(i)
        .map(OptionValue::Int)
        .map_err(|_| range("Atoi", v))
}

/// Dgraph's `UintOptParser`: Go's `strconv.ParseUint(v, 10, 64)`.
pub fn uint_opt_parser(v: &str) -> Result<OptionValue, OptionsError> {
    let Some((_, digits)) = decimal(v, false) else {
        return Err(syntax("ParseUint", v));
    };
    digits
        .parse::<u64>()
        .map(OptionValue::Uint)
        .map_err(|_| range("ParseUint", v))
}

/// Dgraph's `StringOptParser`: the text as it is.
pub fn string_opt_parser(v: &str) -> Result<OptionValue, OptionsError> {
    Ok(OptionValue::Str(v.to_string()))
}

/// Dgraph's `Float64OptParser`: Go's `strconv.ParseFloat(v, 64)` for decimal text, infinities and NaN. Go's
/// hexadecimal floats and underscores are not accepted; no Dgraph option parses floats.
pub fn float64_opt_parser(v: &str) -> Result<OptionValue, OptionsError> {
    parse_float(v).map(OptionValue::Float)
}

/// Dgraph's `Float32OptParser`: Go's `strconv.ParseFloat(v, 32)`, which still returns a float64, rounded to float32
/// precision.
pub fn float32_opt_parser(v: &str) -> Result<OptionValue, OptionsError> {
    let f = parse_float(v)?;
    let n = f as f32;
    if n.is_infinite() && f.is_finite() {
        return Err(range("ParseFloat", v));
    }
    Ok(OptionValue::Float(f64::from(n)))
}

fn parse_float(v: &str) -> Result<f64, OptionsError> {
    let body = v.strip_prefix(['+', '-']).unwrap_or(v).to_ascii_lowercase();
    let special = matches!(body.as_str(), "inf" | "infinity" | "nan");
    let numeric = !body.is_empty()
        && body
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'+' | b'-'))
        && body.bytes().any(|b| b.is_ascii_digit());
    if !special && !numeric {
        return Err(syntax("ParseFloat", v));
    }
    let f: f64 = v.parse().map_err(|_| syntax("ParseFloat", v))?;
    if f.is_infinite() && !special {
        return Err(range("ParseFloat", v));
    }
    Ok(f)
}
