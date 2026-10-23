/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A decoded JSON value as Go's `encoding/json` hands it to Dgraph's JSON parser with `UseNumber`: a number keeps
//! its text (`json.Number`), an object is a map whose last duplicate key wins.

mod json_value_go;

use std::collections::BTreeMap;

use serde_json::value::RawValue;

/// A JSON value, numbers as their text. Go's maps iterate in no order; this one iterates by key.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum JsonValue {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<JsonValue>),
    Object(BTreeMap<String, JsonValue>),
}

impl JsonValue {
    /// Decodes one value serde_json has delimited.
    pub(crate) fn decode(raw: &RawValue) -> Result<JsonValue, serde_json::Error> {
        let text = raw.get();
        Ok(match text.as_bytes().first() {
            Some(b'{') => {
                let m: BTreeMap<String, &RawValue> = serde_json::from_str(text)?;
                JsonValue::Object(
                    m.into_iter()
                        .map(|(k, v)| Ok((k, JsonValue::decode(v)?)))
                        .collect::<Result<_, serde_json::Error>>()?,
                )
            }
            Some(b'[') => {
                let v: Vec<&RawValue> = serde_json::from_str(text)?;
                JsonValue::Array(
                    v.into_iter()
                        .map(JsonValue::decode)
                        .collect::<Result<_, _>>()?,
                )
            }
            Some(b'"') => JsonValue::String(serde_json::from_str(text)?),
            Some(b't') => JsonValue::Bool(true),
            Some(b'f') => JsonValue::Bool(false),
            Some(b'n') => JsonValue::Null,
            _ => JsonValue::Number(text.to_string()),
        })
    }

    /// Go's `%T`-like kind, as `json: cannot unmarshal <kind> into Go value of type ...` names it.
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
