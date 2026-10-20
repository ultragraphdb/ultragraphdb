/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! JSON input as Go's `encoding/json` reads it, before serde_json parses it.

use std::borrow::Cow;

/// The JSON text with every escaped UTF-16 surrogate that is not part of a pair replaced by `�`, as Go's
/// `encoding/json` decodes it; serde_json refuses one.
pub(crate) fn replace_lone_surrogates(text: &str) -> Cow<'_, str> {
    let b = text.as_bytes();
    let hex4 = |i: usize| -> Option<u32> {
        let h = b.get(i..i + 4)?;
        let s = std::str::from_utf8(h).ok()?;
        u32::from_str_radix(s, 16).ok()
    };
    let escaped_u = |i: usize| -> Option<u32> {
        (b.get(i) == Some(&b'\\') && b.get(i + 1) == Some(&b'u'))
            .then(|| hex4(i + 2))
            .flatten()
    };
    let mut out: Option<String> = None;
    let mut copied = 0;
    let (mut i, mut in_string) = (0, false);
    while i < b.len() {
        match (in_string, b[i]) {
            (false, b'"') => in_string = true,
            (false, _) => {}
            (true, b'"') => in_string = false,
            (true, b'\\') if b.get(i + 1) == Some(&b'u') => {
                let Some(u) = hex4(i + 2) else {
                    i += 2;
                    continue;
                };
                let paired = (0xD800..0xDC00).contains(&u)
                    && escaped_u(i + 6).is_some_and(|l| (0xDC00..0xE000).contains(&l));
                if paired {
                    i += 12;
                    continue;
                }
                if (0xD800..0xE000).contains(&u) {
                    let o = out.get_or_insert_with(|| String::with_capacity(text.len()));
                    o.push_str(&text[copied..i]);
                    o.push_str("\\ufffd");
                    copied = i + 6;
                }
                i += 6;
                continue;
            }
            (true, b'\\') => {
                i += 2;
                continue;
            }
            (true, _) => {}
        }
        i += 1;
    }
    match out {
        None => Cow::Borrowed(text),
        Some(mut o) => {
            o.push_str(&text[copied..]);
            Cow::Owned(o)
        }
    }
}
