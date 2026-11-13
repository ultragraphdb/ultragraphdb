/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/value.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

// Arithmetic on decoded text and store input is checked: an overflow here would be a malformed input
// accepted with a wrapped value, so it is refused instead (user decision 2026-11-12; the lint is not in the
// workspace table because elsewhere it flags provably bounded index math).
#![deny(clippy::arithmetic_side_effects)]

use crate::types::type_id::TypeId;
use crate::types::value::Value;
use crate::utils::numbers;
use crate::utils::scalar_types::parse_time;

/// Dgraph's `BytesAsFloatArray`: little-endian float32 values; trailing bytes short of four are ignored.
pub fn bytes_as_float_array(encoded: &[u8]) -> Vec<f32> {
    encoded
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect()
}

/// Dgraph's `FloatArrayAsBytes`: little-endian float32 values.
pub fn float_array_as_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Dgraph's `FloatArrayAsString`: `[a,b,...]`, each Go's shortest float32 `'f'` form.
pub fn float_array_as_string(v: &[f32]) -> String {
    let items: Vec<String> = v.iter().map(|f| numbers::format_f32(*f)).collect();
    format!("[{}]", items.join(","))
}

fn is_sign(d: u8) -> bool {
    d == b'-' || d == b'+'
}

fn strip_sign(s: &[u8]) -> &[u8] {
    match s.split_first() {
        Some((&c, rest)) if is_sign(c) && !rest.is_empty() => rest,
        _ => s,
    }
}

fn contains_any(s: &[u8], chars: &[u8]) -> bool {
    s.iter().any(|c| chars.contains(c))
}

fn check_int(s: &[u8]) -> bool {
    match strip_sign(s) {
        [c, rest @ ..] => c.is_ascii_digit() && !contains_any(rest, b".Ee"),
        [] => false,
    }
}

fn check_float(s: &[u8]) -> bool {
    match strip_sign(s) {
        [b'.', next, ..] => next.is_ascii_digit(),
        [c, rest @ ..] => c.is_ascii_digit() && contains_any(rest, b".Ee"),
        [] => false,
    }
}

fn check_date_time(s: &[u8]) -> bool {
    matches!(s.first_chunk::<5>(), Some([year @ .., b'-']) if year.iter().all(u8::is_ascii_digit))
}

/// Dgraph's `TypeForValue`: the likely type of an untyped value and the parsed value, in Dgraph's order: bool
/// (`true`, `TRUE`, `True` and the false forms), datetime (four digits and `-`), int, float, else default.
pub fn type_for_value(v: &[u8]) -> (TypeId, Option<Value>) {
    let b = v;
    if b.is_empty() {
        return (TypeId::DEFAULT, None);
    }
    if matches!(b.first(), Some(b't' | b'T' | b'f' | b'F')) {
        return match b {
            b"true" | b"TRUE" | b"True" => (TypeId::BOOL, Some(Value::Bool(true))),
            b"false" | b"FALSE" | b"False" => (TypeId::BOOL, Some(Value::Bool(false))),
            _ => (TypeId::DEFAULT, None),
        };
    }
    if check_date_time(b) {
        if let Ok(t) = parse_time(b) {
            return (TypeId::DATETIME, Some(Value::DateTime(t)));
        }
    } else if check_int(b) {
        if let Ok(i) = numbers::parse_int(b) {
            return (TypeId::INT, Some(Value::Int(i)));
        }
    } else if check_float(b)
        && let Ok(f) = numbers::parse_float(b)
    {
        return (TypeId::FLOAT, Some(Value::Float(f)));
    }
    (TypeId::DEFAULT, None)
}
