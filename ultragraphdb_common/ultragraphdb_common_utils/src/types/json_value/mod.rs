/*
 * Copyright 2022 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/encoding/json/v2/arshal_any.go and arshal_default.go (unmarshaling into an interface
 * value with v1's options and UseNumber). Changed for ultragraphdb: translated to Rust; an object keeps its members
 * in order, duplicates included, for decoding into a struct; values nested 10000 deep are built on a stack.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod json_value_clone;
mod json_value_drop;
mod json_value_fmt;
mod json_value_marshal;

use crate::utils::json_wire::{self, NumberState};

/// A JSON value as Go's `encoding/json` decodes it into an `interface{}` with `UseNumber`: `nil`, `bool`, a
/// `json.Number` keeping the number's text, `string`, `[]interface{}` and `map[string]interface{}`.
///
/// A string holds what Go's unquoting gives: each byte that is not UTF-8, and each escaped surrogate that pairs
/// with none, is U+FFFD. An object holds its members in the order of the input, duplicates included, as Go's
/// decoding into a struct reads them; decoded into a Go map, the last of duplicate names wins, which collecting the
/// members into a map gives.
///
/// Go nests values up to 10000 deep, so a value is cloned and dropped from a stack rather than by recursion; that
/// makes the type one that cannot be moved out of by a pattern, and [`std::mem::take`] (which leaves `Null`) takes
/// its parts. `PartialEq` and `Debug` recurse into the value.
#[derive(Debug, Default, PartialEq)]
pub enum JsonValue {
    /// JSON `null`, Go's `nil`.
    #[default]
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number as its text, Go's `json.Number`.
    Number(String),
    /// A string, unquoted.
    String(String),
    /// An array's elements in order.
    Array(Vec<JsonValue>),
    /// An object's members in the order of the input, duplicates included.
    Object(Vec<(String, JsonValue)>),
}

/// An object or array being built, with the name its next member takes.
enum Open {
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>, Option<String>),
}

impl JsonValue {
    /// The kind Go names in `json: cannot unmarshal <kind> into ...`.
    pub fn kind(&self) -> &'static str {
        match self {
            JsonValue::Null => "null",
            JsonValue::Bool(_) => "bool",
            JsonValue::Number(_) => "number",
            JsonValue::String(_) => "string",
            JsonValue::Array(_) => "array",
            JsonValue::Object(_) => "object",
        }
    }

    /// The value of one JSON value the decoder has checked; `b` holds it alone, whitespace aside.
    pub(crate) fn from_valid(b: &[u8]) -> JsonValue {
        let mut open: Vec<Open> = Vec::new();
        let mut i = 0;
        loop {
            i += json_wire::consume_whitespace(&b[i.min(b.len())..]);
            let Some(&c) = b.get(i) else {
                return JsonValue::Null;
            };
            let value = match c {
                b'{' => {
                    open.push(Open::Object(Vec::new(), None));
                    i += 1;
                    continue;
                }
                b'[' => {
                    open.push(Open::Array(Vec::new()));
                    i += 1;
                    continue;
                }
                b',' | b':' => {
                    i += 1;
                    continue;
                }
                b'}' | b']' => {
                    i += 1;
                    match open.pop() {
                        Some(Open::Array(items)) => JsonValue::Array(items),
                        Some(Open::Object(members, _)) => JsonValue::Object(members),
                        None => return JsonValue::Null,
                    }
                }
                b'"' => {
                    let (n, _) = json_wire::consume_string_resumable(&b[i..], 0);
                    let mut s = Vec::with_capacity(n);
                    json_wire::append_unquote(&mut s, &b[i..i + n]);
                    i += n;
                    let s = String::from_utf8(s)
                        .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
                    if let Some(Open::Object(_, name @ None)) = open.last_mut() {
                        *name = Some(s);
                        continue;
                    }
                    JsonValue::String(s)
                }
                b'n' => {
                    i += 4;
                    JsonValue::Null
                }
                b't' => {
                    i += 4;
                    JsonValue::Bool(true)
                }
                b'f' => {
                    i += 5;
                    JsonValue::Bool(false)
                }
                _ => {
                    let (value, n) = number_at(&b[i..]);
                    i += n;
                    value
                }
            };
            match open.last_mut() {
                None => return value,
                Some(Open::Array(items)) => items.push(value),
                Some(Open::Object(members, name)) => {
                    members.push((name.take().unwrap_or_default(), value))
                }
            }
        }
    }
}

/// The number `b` starts with and the bytes it takes, at least one.
fn number_at(b: &[u8]) -> (JsonValue, usize) {
    let (n, _, _) = json_wire::consume_number_resumable(b, 0, NumberState::Init);
    let text = String::from_utf8_lossy(&b[..n]).into_owned();
    (JsonValue::Number(text), n.max(1))
}
