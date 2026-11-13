/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! JSON as this crate's GeoJSON reading and JSON output use it, over `serde_json`: a value with its numbers kept as
//! text, and a string's JSON form.

/// A JSON value with each number kept as the text `serde_json` writes for it, so a reader decides how to take it.
/// An object holds its members by name, the last of duplicate names winning.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) enum JsonValue {
    /// JSON `null`.
    #[default]
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number, as text.
    Number(String),
    /// A string.
    String(String),
    /// An array's elements in order.
    Array(Vec<JsonValue>),
    /// An object's members, by name.
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    /// The value's JSON kind, as an error names it.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            JsonValue::Null => "null",
            JsonValue::Bool(_) => "bool",
            JsonValue::Number(_) => "number",
            JsonValue::String(_) => "string",
            JsonValue::Array(_) => "array",
            JsonValue::Object(_) => "object",
        }
    }
}

impl From<serde_json::Value> for JsonValue {
    fn from(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => JsonValue::Null,
            serde_json::Value::Bool(b) => JsonValue::Bool(b),
            serde_json::Value::Number(n) => JsonValue::Number(n.to_string()),
            serde_json::Value::String(s) => JsonValue::String(s),
            serde_json::Value::Array(items) => {
                JsonValue::Array(items.into_iter().map(JsonValue::from).collect())
            }
            serde_json::Value::Object(members) => JsonValue::Object(
                members
                    .into_iter()
                    .map(|(name, member)| (name, JsonValue::from(member)))
                    .collect(),
            ),
        }
    }
}

/// The JSON document `text` holds, whole; the error is `serde_json`'s text. `serde_json` bounds the nesting it reads,
/// so no document overflows the stack.
pub(crate) fn parse(text: &[u8]) -> Result<JsonValue, String> {
    serde_json::from_slice::<serde_json::Value>(text)
        .map(JsonValue::from)
        .map_err(|e| e.to_string())
}

/// The JSON string holding `s`, each byte sequence that is no UTF-8 as U+FFFD.
pub(crate) fn encode_string(s: &[u8]) -> String {
    serde_json::Value::String(String::from_utf8_lossy(s).into_owned()).to_string()
}

/// What a JSON value of kind `value` gives where `expected` is read, at `field` of `root` when below the root.
pub(crate) fn type_mismatch(value: &str, expected: &str, root: &str, field: &str) -> String {
    if root.is_empty() && field.is_empty() {
        format!("json: cannot read {value} as {expected}")
    } else {
        format!("json: cannot read {value} as {expected} at {root}.{field}")
    }
}
