/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/fmt/print.go (%v of an interface value holding nil, bool, json.Number, string,
 * []interface{} or map[string]interface{}). Changed for ultragraphdb: translated to Rust; written from a stack, so
 * values nested 10000 deep do not recurse.
 * Copyright (c) 2026. Marvin Hansen.
 */

use super::JsonValue;
use super::json_value_marshal::go_map;

/// What is left to write: a value, a delimiter or a map key.
enum Step<'a> {
    Value(&'a JsonValue),
    Text(&'static str),
    Key(&'a str),
}

impl JsonValue {
    /// Go's `%v` of the decoded value: `<nil>`, a number's text, a string unquoted, `[a b]`, `map[k:v]` with the
    /// keys sorted and the last of duplicate names winning.
    pub fn go_v(&self) -> String {
        let mut out = String::new();
        let mut todo = vec![Step::Value(self)];
        while let Some(step) = todo.pop() {
            match step {
                Step::Text(t) => out.push_str(t),
                Step::Key(k) => {
                    out.push_str(k);
                    out.push(':');
                }
                Step::Value(JsonValue::Null) => out.push_str("<nil>"),
                Step::Value(JsonValue::Bool(b)) => out.push_str(if *b { "true" } else { "false" }),
                Step::Value(JsonValue::Number(s) | JsonValue::String(s)) => out.push_str(s),
                Step::Value(JsonValue::Array(items)) => {
                    out.push('[');
                    todo.push(Step::Text("]"));
                    for (i, v) in items.iter().enumerate().rev() {
                        todo.push(Step::Value(v));
                        if i > 0 {
                            todo.push(Step::Text(" "));
                        }
                    }
                }
                Step::Value(JsonValue::Object(members)) => {
                    out.push_str("map[");
                    todo.push(Step::Text("]"));
                    for (i, (k, v)) in go_map(members).into_iter().enumerate().rev() {
                        todo.push(Step::Value(v));
                        todo.push(Step::Key(k));
                        if i > 0 {
                            todo.push(Step::Text(" "));
                        }
                    }
                }
            }
        }
        out
    }
}
