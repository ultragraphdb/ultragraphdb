/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::slice::Iter;

use super::JsonValue;

/// An object or array being copied: the copy so far, the members left, and the name of the member being copied.
enum Open<'a> {
    Array(Vec<JsonValue>, Iter<'a, JsonValue>),
    Object(
        Vec<(String, JsonValue)>,
        Iter<'a, (String, JsonValue)>,
        String,
    ),
}

impl Clone for JsonValue {
    /// Copies the nested values from a stack: Go accepts values 10000 deep, which a recursive copy would overflow a
    /// thread's stack with.
    fn clone(&self) -> Self {
        let mut stack: Vec<Open<'_>> = Vec::new();
        let mut next = Some(self);
        loop {
            // Open the next object or array, or copy the next scalar.
            let mut done = match next.take() {
                Some(JsonValue::Array(items)) => {
                    stack.push(Open::Array(Vec::with_capacity(items.len()), items.iter()));
                    None
                }
                Some(JsonValue::Object(members)) => {
                    let copy = Vec::with_capacity(members.len());
                    stack.push(Open::Object(copy, members.iter(), String::new()));
                    None
                }
                Some(JsonValue::Null) => Some(JsonValue::Null),
                Some(JsonValue::Bool(b)) => Some(JsonValue::Bool(*b)),
                Some(JsonValue::Number(n)) => Some(JsonValue::Number(n.clone())),
                Some(JsonValue::String(s)) => Some(JsonValue::String(s.clone())),
                None => None,
            };
            // Add what is done to the object or array it is in, and go on with the next member.
            loop {
                let Some(top) = stack.last_mut() else {
                    return done.unwrap_or_default();
                };
                if let Some(v) = done.take() {
                    match top {
                        Open::Array(copy, _) => copy.push(v),
                        Open::Object(copy, _, name) => copy.push((std::mem::take(name), v)),
                    }
                }
                let member = match top {
                    Open::Array(_, items) => items.next(),
                    Open::Object(_, members, name) => members.next().map(|(k, v)| {
                        name.clone_from(k);
                        v
                    }),
                };
                if member.is_some() {
                    next = member;
                    break;
                }
                done = match stack.pop() {
                    Some(Open::Array(copy, _)) => Some(JsonValue::Array(copy)),
                    Some(Open::Object(copy, _, _)) => Some(JsonValue::Object(copy)),
                    None => None,
                };
            }
        }
    }
}
