/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use super::JsonValue;

impl Drop for JsonValue {
    /// Drops the nested values from a stack: Go accepts values 10000 deep, which recursive drops would overflow a
    /// thread's stack with.
    fn drop(&mut self) {
        let mut stack: Vec<JsonValue> = match self {
            JsonValue::Array(items) => std::mem::take(items),
            JsonValue::Object(members) => members.drain(..).map(|(_, v)| v).collect(),
            JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) | JsonValue::String(_) => {
                return;
            }
        };
        while let Some(mut v) = stack.pop() {
            match &mut v {
                JsonValue::Array(items) => stack.append(items),
                JsonValue::Object(members) => stack.extend(members.drain(..).map(|(_, v)| v)),
                JsonValue::Null
                | JsonValue::Bool(_)
                | JsonValue::Number(_)
                | JsonValue::String(_) => {}
            }
        }
    }
}
