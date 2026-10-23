/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/value.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::type_id::TypeId;
use crate::types::value::Value;
use crate::utils::scalar_types::parse_time;
use ultragraphdb_common_utils::go_strconv;

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
    let items: Vec<String> = v
        .iter()
        .map(|f| go_strconv::format_float(f64::from(*f), b'f', 32))
        .collect();
    format!("[{}]", items.join(","))
}

fn is_sign(d: u8) -> bool {
    d == b'-' || d == b'+'
}

fn strip_sign(s: &[u8]) -> &[u8] {
    if is_sign(s[0]) && s.len() > 1 {
        &s[1..]
    } else {
        s
    }
}

fn contains_any(s: &[u8], chars: &[u8]) -> bool {
    s.iter().any(|c| chars.contains(c))
}

fn check_int(s: &[u8]) -> bool {
    let s = strip_sign(s);
    s[0].is_ascii_digit() && !contains_any(&s[1..], b".Ee")
}

fn check_float(s: &[u8]) -> bool {
    let s = strip_sign(s);
    if s[0] == b'.' && s.len() > 1 {
        return s[1].is_ascii_digit();
    }
    s[0].is_ascii_digit() && contains_any(&s[1..], b".Ee")
}

fn check_date_time(s: &[u8]) -> bool {
    s.len() >= 5 && s[..4].iter().all(u8::is_ascii_digit) && s[4] == b'-'
}

/// Dgraph's `TypeForValue`: the likely type of an untyped value and the parsed value, in Dgraph's order: bool
/// (`true`, `TRUE`, `True` and the false forms), datetime (four digits and `-`), int, float, else default.
pub fn type_for_value(v: &[u8]) -> (TypeId, Option<Value>) {
    let b = v;
    if b.is_empty() {
        return (TypeId::DEFAULT, None);
    }
    if matches!(b[0], b't' | b'T' | b'f' | b'F') {
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
        if let Ok(i) = go_strconv::parse_int(b, 10) {
            return (TypeId::INT, Some(Value::Int(i)));
        }
    } else if check_float(b)
        && let Ok(f) = go_strconv::parse_float(b, 64)
    {
        return (TypeId::FLOAT, Some(Value::Float(f)));
    }
    (TypeId::DEFAULT, None)
}
