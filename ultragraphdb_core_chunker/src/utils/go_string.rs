/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Go strings hold any bytes; a Rust string, and the strings of the dgo API's messages, hold UTF-8 only.

use std::borrow::Cow;

/// The bytes as text, each byte that is not part of a UTF-8 sequence replaced by U+FFFD. That is how Go reads them:
/// ranging over a string, decoding a rune, unquoting a literal or writing JSON, Go takes every such byte for one
/// U+FFFD (`String::from_utf8_lossy` replaces a truncated sequence by a single one instead).
pub(crate) fn go_string(b: &[u8]) -> Cow<'_, str> {
    if let Ok(s) = std::str::from_utf8(b) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(b.len() + 8);
    for chunk in b.utf8_chunks() {
        out.push_str(chunk.valid());
        chunk.invalid().iter().for_each(|_| out.push('\u{fffd}'));
    }
    Cow::Owned(out)
}
