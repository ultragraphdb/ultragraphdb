/*
 * Copyright 2010 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/encoding/json/v2_scanner.go (Valid, checkValid), v2/arshal.go (Unmarshal with v1's
 * legacy semantics) and internal/jsonwire/encode.go (AppendQuote with v1's escaping). Changed for ultragraphdb:
 * translated to Rust; Unmarshal decodes into a JsonValue, numbers kept as text as json.Number and
 * json.RawMessage keep them; a string's invalid UTF-8 is written as v1 wrote it.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Go's `encoding/json` as Dgraph runs it on Go 1.27, where it is built on `encoding/json/jsontext`: `Valid`,
//! `Unmarshal` into an `interface{}` and `Marshal` of a string. The stream decoder is
//! [`JsonDecoder`](crate::JsonDecoder) and the decoded value [`JsonValue`](crate::JsonValue).
//!
//! Errors carry Go's texts and offsets: `invalid character 'x' looking for beginning of value`,
//! `unexpected end of JSON input`, `exceeded max depth` for values nested deeper than 10000.

use ultragraphdb_error_json::{JsonError, JsonErrorEnum};

use crate::types::json_decoder::JsonDecoder;
use crate::types::json_value::JsonValue;

/// Go's `checkValid`, the syntax check `json.Unmarshal` runs first: one JSON value, whitespace around it.
fn check_valid(data: &[u8]) -> Result<(usize, usize), JsonError> {
    let mut d = JsonDecoder::of_bytes(data);
    let range = d.read_value().map_err(|e| match e.0 {
        // Nothing but whitespace: the input ended where a value should start.
        JsonErrorEnum::Eof => JsonError::Syntax(
            "unexpected end of JSON input".to_string(),
            data.len() as i64,
        ),
        _ => e,
    })?;
    d.check_eof()?;
    Ok(range)
}

/// Go's `json.Valid`: whether `data` is one JSON value, whitespace around it.
pub fn valid(data: &[u8]) -> bool {
    check_valid(data).is_ok()
}

/// Go's `json.Unmarshal` into an `interface{}`, numbers kept as their text: the syntax of all of `data` is
/// checked before it is decoded.
///
/// # Errors
///
/// [`JsonErrorEnum::Syntax`] with Go's text and offset, `unexpected end of JSON input` for input that ends inside a
/// value or holds none.
pub fn unmarshal(data: &[u8]) -> Result<JsonValue, JsonError> {
    let (start, end) = check_valid(data)?;
    Ok(JsonValue::from_valid(&data[start..end]))
}

/// Go's `json.Marshal` of a string: the quote, the backslash and control characters escaped (`\b`, `\f`, `\n`,
/// `\r`, `\t` by name), the HTML characters `<`, `>`, `&` and U+2028 and U+2029 as `\u` escapes, and each byte
/// that is not UTF-8 as the escape `�`, as Go's v1 wrote it.
pub fn encode_string(s: &[u8]) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for chunk in s.utf8_chunks() {
        for c in chunk.valid().chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\u{8}' => out.push_str("\\b"),
                '\u{c}' => out.push_str("\\f"),
                '<' | '>' | '&' | '\u{2028}' | '\u{2029}' => {
                    out.push_str(&format!("\\u{:04x}", c as u32))
                }
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        chunk.invalid().iter().for_each(|_| out.push_str("\\ufffd"));
    }
    out.push('"');
    out
}
