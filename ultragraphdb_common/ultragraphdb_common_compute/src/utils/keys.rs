/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The key past every key starting with `prefix`: the prefix with its last byte below 255 raised, those after it dropped; empty
/// when no such key exists.
#[must_use]
pub fn prefix_end(prefix: &[u8]) -> Vec<u8> {
    let mut end = prefix.to_vec();
    while let Some(last) = end.pop() {
        if let Some(raised) = last.checked_add(1) {
            end.push(raised);
            return end;
        }
    }
    end
}

/// The key past `key` alone: `key` followed by a zero byte.
pub(crate) fn past_alone(key: &[u8]) -> Vec<u8> {
    let mut past = Vec::with_capacity(key.len().saturating_add(1));
    past.extend_from_slice(key);
    past.push(0);
    past
}

/// `key` as a message may name it.
pub(crate) fn hex(key: &[u8]) -> String {
    use std::fmt::Write as _;

    key.iter().fold(
        String::with_capacity(key.len().saturating_mul(2)),
        |mut text, byte| {
            // Writing to a string does not fail.
            let _wrote: std::fmt::Result = write!(text, "{byte:02x}");
            text
        },
    )
}
