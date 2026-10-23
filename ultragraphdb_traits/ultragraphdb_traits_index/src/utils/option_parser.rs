/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/options/option_parser.go. Changed for ultragraphdb: translated to Rust over Go's
 * strconv parsers, ultragraphdb_common_utils::go_strconv.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_common_utils::go_strconv;
use ultragraphdb_error_options::OptionsError;

use crate::types::option_value::OptionValue;

/// The option's text did not parse, with Go's `strconv` message.
fn parse_error(e: impl ToString) -> OptionsError {
    OptionsError::Parse(e.to_string())
}

/// Dgraph's `IntOptParser`: Go's `strconv.Atoi`.
pub fn int_opt_parser(v: &str) -> Result<OptionValue, OptionsError> {
    go_strconv::atoi(v)
        .map(OptionValue::Int)
        .map_err(parse_error)
}

/// Dgraph's `UintOptParser`: Go's `strconv.ParseUint(v, 10, 64)`.
pub fn uint_opt_parser(v: &str) -> Result<OptionValue, OptionsError> {
    go_strconv::parse_uint(v, 10)
        .map(OptionValue::Uint)
        .map_err(parse_error)
}

/// Dgraph's `StringOptParser`: the text as it is.
pub fn string_opt_parser(v: &str) -> Result<OptionValue, OptionsError> {
    Ok(OptionValue::Str(v.to_string()))
}

/// Dgraph's `Float64OptParser`: Go's `strconv.ParseFloat(v, 64)`.
pub fn float64_opt_parser(v: &str) -> Result<OptionValue, OptionsError> {
    go_strconv::parse_float(v, 64)
        .map(OptionValue::Float)
        .map_err(parse_error)
}

/// Dgraph's `Float32OptParser`: Go's `strconv.ParseFloat(v, 32)`, which still returns a float64, rounded to float32
/// precision.
pub fn float32_opt_parser(v: &str) -> Result<OptionValue, OptionsError> {
    go_strconv::parse_float(v, 32)
        .map(OptionValue::Float)
        .map_err(parse_error)
}
