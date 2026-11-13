/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Numbers and booleans as text, over the standard library and the XSD lexical forms of `oxsdatatypes`.

use oxsdatatypes::{Boolean, Double};

use crate::errors::types_error::TypesError;

/// `data` as text.
fn text(data: &[u8]) -> Result<&str, TypesError> {
    std::str::from_utf8(data).map_err(|_| TypesError::number("a number that is no UTF-8"))
}

/// The int64 `data` writes in decimal, with an optional sign.
pub(crate) fn parse_int(data: &[u8]) -> Result<i64, TypesError> {
    Ok(text(data)?.parse::<i64>()?)
}

/// The double `data` writes: decimal digits with an optional point and exponent, `inf`, `infinity` and `nan` in any
/// case. Digits past a double's range are refused.
pub(crate) fn parse_float(data: &[u8]) -> Result<f64, TypesError> {
    let text = text(data)?;
    let f = text.parse::<f64>()?;
    // The standard library reads digits past a double's range as an infinity; only a text naming one gives one.
    if f.is_infinite() && !names_infinity(text) {
        return Err(TypesError::number(format!(
            "{text:?} is outside a double's range"
        )));
    }
    Ok(f)
}

/// The boolean `data` writes in XSD's forms: `true`, `false`, `1` or `0`.
pub(crate) fn parse_bool(data: &[u8]) -> Result<bool, TypesError> {
    Ok(bool::from(text(data)?.parse::<Boolean>()?))
}

/// Whether `text` names an infinity, as against digits past a type's range, which the standard library also reads as
/// one.
fn names_infinity(text: &str) -> bool {
    text.trim_start_matches(['+', '-'])
        .get(..3)
        .is_some_and(|start| start.eq_ignore_ascii_case("inf"))
}

/// The float `text` writes, as [`parse_float`] reads a double; digits past a float's range are refused.
pub(crate) fn parse_f32(text: &str) -> Result<f32, TypesError> {
    let f = text.parse::<f32>()?;
    if f.is_infinite() && !names_infinity(text) {
        return Err(TypesError::number(format!(
            "{text:?} is outside a float's range"
        )));
    }
    Ok(f)
}

/// A double's text: the shortest digits that read back to it, in exponent form (`2.5E-7`) below 1e-6 and from 1e21,
/// `INF` and `-INF` for the infinities and `NaN`.
pub(crate) fn format_float(f: f64) -> String {
    let magnitude = f.abs();
    if f.is_finite() && magnitude != 0.0 && !(1e-6..1e21).contains(&magnitude) {
        format!("{f:E}")
    } else {
        Double::from(f).to_string()
    }
}

/// A float's text, as [`format_float`] writes a double's, from the float's own shortest digits.
pub(crate) fn format_f32(f: f32) -> String {
    let magnitude = f.abs();
    if f.is_finite() && magnitude != 0.0 && !(1e-6..1e21).contains(&magnitude) {
        format!("{f:E}")
    } else if f.is_infinite() {
        format_float(f64::from(f))
    } else {
        f.to_string()
    }
}
