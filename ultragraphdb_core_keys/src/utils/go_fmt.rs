/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The parts of Go's formatting and case mapping that Dgraph's messages and name checks rely on.

use std::fmt::Write;

/// A byte slice as Go's `%v` prints it: `[1 2 3]`.
pub(crate) fn go_bytes(bytes: &[u8]) -> String {
    let items: Vec<String> = bytes.iter().map(u8::to_string).collect();
    format!("[{}]", items.join(" "))
}

/// Bytes as Go's `%q` (`strconv.Quote`) prints them: double-quoted, a byte of invalid UTF-8 as `\xHH`.
///
/// Go keeps a rune that `strconv.IsPrint` accepts and escapes the rest. This port escapes control characters,
/// whitespace other than the space, format characters (Unicode category Cf) and private-use characters, and keeps
/// every other rune; it differs from Go only on code points unassigned in Unicode, which Go escapes.
pub(crate) fn go_quote(bytes: &[u8]) -> String {
    let mut out = String::from("\"");
    for chunk in bytes.utf8_chunks() {
        for c in chunk.valid().chars() {
            push_escaped(&mut out, c);
        }
        for b in chunk.invalid() {
            write!(out, "\\x{b:02x}").expect("writing to a String");
        }
    }
    out.push('"');
    out
}

fn push_escaped(out: &mut String, c: char) {
    match c {
        '"' | '\\' => {
            out.push('\\');
            out.push(c);
        }
        _ if is_print(c) => out.push(c),
        '\u{07}' => out.push_str("\\a"),
        '\u{08}' => out.push_str("\\b"),
        '\u{0c}' => out.push_str("\\f"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        '\u{0b}' => out.push_str("\\v"),
        _ if c < ' ' || c == '\u{7f}' => {
            write!(out, "\\x{:02x}", u32::from(c)).expect("writing to a String")
        }
        _ if u32::from(c) < 0x10000 => {
            write!(out, "\\u{:04x}", u32::from(c)).expect("writing to a String")
        }
        _ => write!(out, "\\U{:08x}", u32::from(c)).expect("writing to a String"),
    }
}

// Unicode's format characters (category Cf).
const FORMAT: [(u32, u32); 21] = [
    (0x00AD, 0x00AD),
    (0x0600, 0x0605),
    (0x061C, 0x061C),
    (0x06DD, 0x06DD),
    (0x070F, 0x070F),
    (0x0890, 0x0891),
    (0x08E2, 0x08E2),
    (0x180E, 0x180E),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x2064),
    (0x2066, 0x206F),
    (0xFEFF, 0xFEFF),
    (0xFFF9, 0xFFFB),
    (0x110BD, 0x110BD),
    (0x110CD, 0x110CD),
    (0x13430, 0x1343F),
    (0x1BCA0, 0x1BCA3),
    (0x1D173, 0x1D17A),
    (0xE0001, 0xE0001),
    (0xE0020, 0xE007F),
];

// Unicode's private-use characters (category Co).
const PRIVATE_USE: [(u32, u32); 3] = [(0xE000, 0xF8FF), (0xF0000, 0xFFFFD), (0x100000, 0x10FFFD)];

fn is_print(c: char) -> bool {
    if c.is_ascii() {
        return c == ' ' || c.is_ascii_graphic();
    }
    let cp = u32::from(c);
    let within = |ranges: &[(u32, u32)]| ranges.iter().any(|&(lo, hi)| (lo..=hi).contains(&cp));
    !(c.is_control() || c.is_whitespace() || within(&FORMAT) || within(&PRIVATE_USE))
}

/// Go's `strings.ToLower`, which maps each rune by its simple lowercase mapping. Rust's `to_lowercase` uses the
/// full mapping instead, which differs for `İ` (U+0130, `i` in Go) and for a final `Σ` (always `σ` in Go).
pub(crate) fn go_to_lower(s: &str) -> String {
    s.chars()
        .flat_map(|c| {
            if c == '\u{130}' {
                'i'.to_lowercase()
            } else {
                c.to_lowercase()
            }
        })
        .collect()
}
