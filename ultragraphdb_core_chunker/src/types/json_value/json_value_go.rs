/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! How Go prints and re-encodes a decoded JSON value: `fmt`'s `%v` and `json.Marshal`.

use super::JsonValue;

impl JsonValue {
    /// Go's `%v` of the decoded value: `<nil>`, a number's text, a string unquoted, `[a b]`, `map[k:v]` with the
    /// keys sorted.
    pub(crate) fn go_v(&self) -> String {
        let join = |parts: Vec<String>| parts.join(" ");
        match self {
            JsonValue::Null => "<nil>".to_string(),
            JsonValue::Bool(b) => b.to_string(),
            JsonValue::Number(n) => n.clone(),
            JsonValue::String(s) => s.clone(),
            JsonValue::Array(items) => {
                format!("[{}]", join(items.iter().map(JsonValue::go_v).collect()))
            }
            JsonValue::Object(m) => format!(
                "map[{}]",
                join(m.iter().map(|(k, v)| format!("{k}:{}", v.go_v())).collect())
            ),
        }
    }

    /// Go's `json.Marshal` of the value: object keys sorted, a number as its text.
    pub(crate) fn marshal(&self) -> String {
        let mut out = String::new();
        self.write_json(&mut out);
        out
    }

    fn write_json(&self, out: &mut String) {
        match self {
            JsonValue::Null => out.push_str("null"),
            JsonValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            JsonValue::Number(n) => out.push_str(n),
            JsonValue::String(s) => out.push_str(&serde_json::Value::from(s.as_str()).to_string()),
            JsonValue::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write_json(out);
                }
                out.push(']');
            }
            JsonValue::Object(m) => {
                out.push('{');
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&serde_json::Value::from(k.as_str()).to_string());
                    out.push(':');
                    v.write_json(out);
                }
                out.push('}');
            }
        }
    }
}
