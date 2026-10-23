/*
 * Copyright 2010 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/encoding/json/v2/arshal_any.go (marshalValueAny with v1's options) and v2_decode.go
 * (Number.MarshalJSONTo). Changed for ultragraphdb: translated to Rust; written from a stack, so values nested
 * 10000 deep do not recurse.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::BTreeMap;

use super::JsonValue;
use crate::utils::go_json::encode_string;

/// What is left to write: a value, a delimiter or an object member's name.
enum Step<'a> {
    Value(&'a JsonValue),
    Text(&'static str),
    Name(&'a str),
}

/// An object's members as Go's map holds them: the last of duplicate names wins, in the order of the names.
pub(super) fn go_map(members: &[(String, JsonValue)]) -> Vec<(&str, &JsonValue)> {
    let map: BTreeMap<&str, &JsonValue> = members.iter().map(|(k, v)| (k.as_str(), v)).collect();
    map.into_iter().collect()
}

/// Pushes the steps of a list, last first: each item preceded by `sep` but the first.
fn push_items<'a>(
    todo: &mut Vec<Step<'a>>,
    items: Vec<(Option<&'a str>, &'a JsonValue)>,
    sep: &'static str,
) {
    for (i, (name, v)) in items.into_iter().enumerate().rev() {
        todo.push(Step::Value(v));
        if let Some(name) = name {
            todo.push(Step::Name(name));
        }
        if i > 0 {
            todo.push(Step::Text(sep));
        }
    }
}

impl JsonValue {
    /// Go's `json.Marshal` of the decoded value: an object's names sorted, the last of duplicates winning; a
    /// number as its text; a string escaped as [`encode_string`](crate::go_json::encode_string) escapes it.
    pub fn marshal(&self) -> String {
        let mut out = String::new();
        let mut todo = vec![Step::Value(self)];
        while let Some(step) = todo.pop() {
            match step {
                Step::Text(t) => out.push_str(t),
                Step::Name(name) => {
                    out.push_str(&encode_string(name.as_bytes()));
                    out.push(':');
                }
                Step::Value(JsonValue::Null) => out.push_str("null"),
                Step::Value(JsonValue::Bool(b)) => out.push_str(if *b { "true" } else { "false" }),
                // An empty json.Number marshals as 0.
                Step::Value(JsonValue::Number(n)) => {
                    out.push_str(if n.is_empty() { "0" } else { n })
                }
                Step::Value(JsonValue::String(s)) => out.push_str(&encode_string(s.as_bytes())),
                Step::Value(JsonValue::Array(items)) => {
                    out.push('[');
                    todo.push(Step::Text("]"));
                    push_items(&mut todo, items.iter().map(|v| (None, v)).collect(), ",");
                }
                Step::Value(JsonValue::Object(members)) => {
                    out.push('{');
                    todo.push(Step::Text("}"));
                    let items = go_map(members)
                        .into_iter()
                        .map(|(k, v)| (Some(k), v))
                        .collect();
                    push_items(&mut todo, items, ",");
                }
            }
        }
        out
    }
}
