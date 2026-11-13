/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/options/option_parser.go. Changed for ultragraphdb: translated to Rust over the
 * standard library's number parsers.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_error_options::OptionsError;

use crate::types::option_value::OptionValue;

/// The option's text did not parse, with the standard library's message.
fn parse_error(e: &impl ToString) -> OptionsError {
    OptionsError::parse(e.to_string())
}

/// An option holding a signed integer in decimal.
pub fn int_opt_parser(v: &str) -> Result<OptionValue, OptionsError> {
    v.parse::<i64>()
        .map(OptionValue::Int)
        .map_err(|e| parse_error(&e))
}

/// An option holding an unsigned integer in decimal.
pub fn uint_opt_parser(v: &str) -> Result<OptionValue, OptionsError> {
    v.parse::<u64>()
        .map(OptionValue::Uint)
        .map_err(|e| parse_error(&e))
}

/// An option holding its text as it is.
pub fn string_opt_parser(v: &str) -> Result<OptionValue, OptionsError> {
    Ok(OptionValue::Str(v.to_string()))
}

/// The text of a float too large for its type, which the standard library reads as an infinity.
const OUT_OF_RANGE: &str = "number too large to fit in target type";

/// `f`, read from `v`, refused when it is an infinity `v` does not name: digits past the type's range.
fn in_range(v: &str, f: f64) -> Result<OptionValue, OptionsError> {
    let named = v
        .trim_start_matches(['+', '-'])
        .get(..3)
        .is_some_and(|start| start.eq_ignore_ascii_case("inf"));
    if f.is_infinite() && !named {
        return Err(parse_error(&OUT_OF_RANGE));
    }
    Ok(OptionValue::Float(f))
}

/// An option holding a double.
pub fn float64_opt_parser(v: &str) -> Result<OptionValue, OptionsError> {
    v.parse::<f64>()
        .map_err(|e| parse_error(&e))
        .and_then(|f| in_range(v, f))
}

/// An option holding a float: the text's nearest `f32`, widened.
pub fn float32_opt_parser(v: &str) -> Result<OptionValue, OptionsError> {
    v.parse::<f32>()
        .map_err(|e| parse_error(&e))
        .and_then(|f| in_range(v, f64::from(f)))
}
