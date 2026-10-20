/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26, src/strconv/quote.go and src/fmt/format.go (fmtUnicode). Changed for ultragraphdb:
 * translated to Rust; `IsPrint` approximated from Rust's character classes.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// Go's `strconv.IsPrint`, approximated: Rust has no general category, so controls, separators other than the
/// ASCII space, the format characters and the private use areas are taken as unprintable, and everything else
/// as printable.
pub(crate) fn is_print(r: char) -> bool {
    if r == ' ' {
        return true;
    }
    if r.is_control() || r.is_whitespace() {
        return false;
    }
    !matches!(r,
        '\u{00AD}' | '\u{0600}'..='\u{0605}' | '\u{061C}' | '\u{06DD}' | '\u{070F}' | '\u{180E}'
        | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{206F}' | '\u{FEFF}'
        | '\u{FFF9}'..='\u{FFFB}' | '\u{E000}'..='\u{F8FF}' | '\u{F0000}'..='\u{10FFFF}')
}

/// Go's `strconv.Quote`, as `%q` writes a string.
pub(crate) fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for r in s.chars() {
        match r {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ if is_print(r) => out.push(r),
            '\u{7}' => out.push_str("\\a"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{b}' => out.push_str("\\v"),
            _ if (r as u32) < 0x20 || r == '\u{7f}' => {
                out.push_str(&format!("\\x{:02x}", r as u32))
            }
            _ if (r as u32) < 0x10000 => out.push_str(&format!("\\u{:04x}", r as u32)),
            _ => out.push_str(&format!("\\U{:08x}", r as u32)),
        }
    }
    out.push('"');
    out
}

/// Go's `%#U` of a rune, `U+0041 'A'`; the end of the input is Go's rune -1, `U+FFFFFFFFFFFFFFFF`.
pub(crate) fn unicode_hash(r: Option<char>) -> String {
    match r {
        None => "U+FFFFFFFFFFFFFFFF".to_string(),
        Some(c) if is_print(c) => format!("U+{:04X} '{c}'", c as u32),
        Some(c) => format!("U+{:04X}", c as u32),
    }
}
