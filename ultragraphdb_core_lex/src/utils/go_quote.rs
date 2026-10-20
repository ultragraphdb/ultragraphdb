/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Go's `%q` verb, which Dgraph's lexer uses in an item's `String` and in its IRI error: `strconv.Quote` for a
//! string and `strconv.QuoteRune` for a rune.
//!
//! Go escapes every rune `unicode.IsPrint` rejects. ASCII follows Go exactly. Beyond ASCII, a rune counts as
//! unprintable when it is a control (Cc), a separator (Zs, Zl, Zp), a format character (Cf) or private use (Co);
//! Go also escapes unassigned code points (Cn), which this approximation prints as they are.

/// Format characters (Cf) beyond ASCII, Unicode 15.
const FORMAT_CHARS: [(char, char); 21] = [
    ('\u{00AD}', '\u{00AD}'),
    ('\u{0600}', '\u{0605}'),
    ('\u{061C}', '\u{061C}'),
    ('\u{06DD}', '\u{06DD}'),
    ('\u{070F}', '\u{070F}'),
    ('\u{0890}', '\u{0891}'),
    ('\u{08E2}', '\u{08E2}'),
    ('\u{180E}', '\u{180E}'),
    ('\u{200B}', '\u{200F}'),
    ('\u{202A}', '\u{202E}'),
    ('\u{2060}', '\u{2064}'),
    ('\u{2066}', '\u{206F}'),
    ('\u{FEFF}', '\u{FEFF}'),
    ('\u{FFF9}', '\u{FFFB}'),
    ('\u{110BD}', '\u{110BD}'),
    ('\u{110CD}', '\u{110CD}'),
    ('\u{13430}', '\u{1343F}'),
    ('\u{1BCA0}', '\u{1BCA3}'),
    ('\u{1D173}', '\u{1D17A}'),
    ('\u{E0001}', '\u{E0001}'),
    ('\u{E0020}', '\u{E007F}'),
];

/// Private use (Co).
const PRIVATE_USE: [(char, char); 3] = [
    ('\u{E000}', '\u{F8FF}'),
    ('\u{F0000}', '\u{FFFFD}'),
    ('\u{100000}', '\u{10FFFD}'),
];

const LOWER_HEX: &[u8; 16] = b"0123456789abcdef";

/// `strconv.Quote`: the string in double quotes, with Go's escapes.
pub(crate) fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    s.chars().for_each(|r| push_escaped(&mut out, r, '"'));
    out.push('"');
    out
}

/// `strconv.QuoteRune`: the rune in single quotes, with Go's escapes.
pub(crate) fn quote_rune(r: char) -> String {
    let mut out = String::with_capacity(6);
    out.push('\'');
    push_escaped(&mut out, r, '\'');
    out.push('\'');
    out
}

fn push_escaped(out: &mut String, r: char, quote: char) {
    if r == quote || r == '\\' {
        out.push('\\');
        out.push(r);
        return;
    }
    if is_print(r) {
        out.push(r);
        return;
    }
    match r {
        '\u{07}' => out.push_str("\\a"),
        '\u{08}' => out.push_str("\\b"),
        '\u{0C}' => out.push_str("\\f"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        '\u{0B}' => out.push_str("\\v"),
        r if r < ' ' || r == '\u{7F}' => push_hex(out, "\\x", r as u32, 2),
        r if (r as u32) < 0x10000 => push_hex(out, "\\u", r as u32, 4),
        r => push_hex(out, "\\U", r as u32, 8),
    }
}

fn push_hex(out: &mut String, prefix: &str, value: u32, digits: u32) {
    out.push_str(prefix);
    (0..digits)
        .rev()
        .map(|i| LOWER_HEX[((value >> (4 * i)) & 0xF) as usize] as char)
        .for_each(|c| out.push(c));
}

fn is_print(r: char) -> bool {
    if r.is_ascii() {
        return (' '..='~').contains(&r);
    }
    let in_ranges = |ranges: &[(char, char)]| ranges.iter().any(|&(lo, hi)| (lo..=hi).contains(&r));
    !(r.is_control() || r.is_whitespace() || in_ranges(&FORMAT_CHARS) || in_ranges(&PRIVATE_USE))
}
