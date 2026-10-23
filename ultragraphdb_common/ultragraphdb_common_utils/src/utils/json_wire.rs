/*
 * Copyright 2023 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/encoding/json/internal/jsonwire/decode.go and src/unicode/utf8/utf8.go (FullRune).
 * Changed for ultragraphdb: translated to Rust; strings are consumed as Go's v1 decoder consumes them, with
 * AllowInvalidUTF8, so neither invalid UTF-8 nor unpaired surrogate escapes are syntax errors.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The stateless consumption of JSON tokens from a buffer, Go's `jsonwire`. A function reports how many bytes it
//! consumed and whether it stopped on an error; [`ScanError::UnexpectedEof`] means the buffer may hold only a
//! prefix of the token, and the resumable functions continue from what they returned once the buffer holds more.

use crate::types::scan_error::ScanError;
use crate::utils::go_utf8;

/// JSON whitespace per RFC 8259: space, tab, carriage return and line feed.
pub(crate) fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n')
}

/// Go's `ConsumeWhitespace`: the length of the leading whitespace.
pub(crate) fn consume_whitespace(b: &[u8]) -> usize {
    b.iter().take_while(|&&c| is_space(c)).count()
}

/// Go's `ConsumeLiteral`: `null`, `false` or `true`, or how far it matched.
pub(crate) fn consume_literal(b: &[u8], lit: &str) -> (usize, Option<ScanError>) {
    let l = lit.as_bytes();
    if let Some(i) = b.iter().zip(l).position(|(c, w)| c != w) {
        let expecting = crate::utils::go_strconv::quote_rune(char::from(l[i]));
        let where_ = format!("in literal {lit} (expecting {expecting})");
        return (i, Some(ScanError::invalid_character(&b[i..], where_)));
    }
    if b.len() < l.len() {
        return (b.len(), Some(ScanError::UnexpectedEof));
    }
    (l.len(), None)
}

/// Go's `hasEscapedUTF16Prefix`: whether `b` may be the truncated prefix of a `￿` escape, of a low surrogate
/// when `lower_surrogate_half`.
fn has_escaped_utf16_prefix(b: &[u8], lower_surrogate_half: bool) -> bool {
    b.iter().take(6).enumerate().all(|(i, &c)| match i {
        0 => c == b'\\',
        1 => c == b'u',
        2 if lower_surrogate_half && !matches!(c, b'd' | b'D') => false,
        3 if lower_surrogate_half && !matches!(c, b'c'..=b'f' | b'C'..=b'F') => false,
        _ => c.is_ascii_hexdigit(),
    })
}

/// Go's `parseHexUint16`: four hexadecimal digits.
fn parse_hex_u16(b: &[u8]) -> Option<u32> {
    (b.len() == 4 && b.iter().all(u8::is_ascii_hexdigit)).then(|| {
        b.iter()
            .fold(0, |v, &c| v * 16 + char::from(c).to_digit(16).unwrap_or(0))
    })
}

/// Go's `utf8.FullRune`: whether `p` begins with a whole UTF-8 sequence, or with bytes no continuation can make
/// one, which decode as one invalid byte.
pub(crate) fn full_rune(p: &[u8]) -> bool {
    let Some(&b0) = p.first() else {
        return false;
    };
    let (size, lo, hi) = match b0 {
        0xC2..=0xDF => (2, 0x80, 0xBF),
        0xE0 => (3, 0xA0, 0xBF),
        0xED => (3, 0x80, 0x9F),
        0xE1..=0xEF => (3, 0x80, 0xBF),
        0xF0 => (4, 0x90, 0xBF),
        0xF4 => (4, 0x80, 0x8F),
        0xF1..=0xF3 => (4, 0x80, 0xBF),
        _ => return true,
    };
    if p.len() >= size {
        return true;
    }
    match p.get(1) {
        Some(&c) if !(lo..=hi).contains(&c) => true,
        _ => p.get(2).is_some_and(|&c| !(0x80..=0xBF).contains(&c)),
    }
}

/// Go's `ConsumeStringResumable` without UTF-8 validation: a JSON string from `b`, continuing at `resume` when it
/// is not zero.
pub(crate) fn consume_string_resumable(b: &[u8], resume: usize) -> (usize, Option<ScanError>) {
    let mut n = match b.first() {
        _ if resume > 0 => resume,
        None => return (0, Some(ScanError::UnexpectedEof)),
        Some(b'"') => 1,
        Some(_) => {
            return (
                0,
                Some(ScanError::invalid_character(
                    b,
                    "at start of string (expecting '\"')",
                )),
            );
        }
    };
    while n < b.len() {
        // Long runs of unescaped ASCII.
        n += b[n..]
            .iter()
            .take_while(|&&c| (b' '..0x80).contains(&c) && c != b'\\' && c != b'"')
            .count();
        let Some(&c) = b.get(n) else {
            return (n, Some(ScanError::UnexpectedEof));
        };
        if c == b'"' {
            return (n + 1, None);
        }
        let (r, rn) = go_utf8::decode_rune(&b[n..]);
        if rn > 1 {
            n += rn;
        } else if c == b'\\' {
            let resume = n;
            let Some(&e) = b.get(n + 1) else {
                return (resume, Some(ScanError::UnexpectedEof));
            };
            match e {
                b'/' | b'"' | b'\\' | b'b' | b'f' | b'n' | b'r' | b't' => n += 2,
                b'u' => {
                    if b.len() < n + 6 {
                        if has_escaped_utf16_prefix(&b[n..], false) {
                            return (resume, Some(ScanError::UnexpectedEof));
                        }
                        return (n, Some(ScanError::invalid_escape(&b[n..])));
                    }
                    if parse_hex_u16(&b[n + 2..n + 6]).is_none() {
                        return (n, Some(ScanError::invalid_escape(&b[n..n + 6])));
                    }
                    n += 6;
                }
                _ => return (n, Some(ScanError::invalid_escape(&b[n..n + 2]))),
            }
        } else if r == go_utf8::RUNE_ERROR {
            // A byte that is not UTF-8, unless more input may complete it.
            if !full_rune(&b[n..]) {
                return (n, Some(ScanError::UnexpectedEof));
            }
            n += 1;
        } else {
            // What is left of ASCII is a control character.
            return (
                n,
                Some(ScanError::invalid_character(
                    &b[n..],
                    "in string (expecting non-control character)",
                )),
            );
        }
    }
    (n, Some(ScanError::UnexpectedEof))
}

/// Go's `utf16.IsSurrogate`.
fn is_surrogate(r: u32) -> bool {
    (0xD800..0xE000).contains(&r)
}

/// Go's `AppendUnquote` of a complete JSON string, its errors dropped as `UnquoteMayCopy` drops them: each byte
/// that is not UTF-8 and each unpaired surrogate escape becomes U+FFFD.
pub(crate) fn append_unquote(dst: &mut Vec<u8>, src: &[u8]) {
    let push_rune = |dst: &mut Vec<u8>, r: u32| {
        let c = char::from_u32(r).unwrap_or(char::REPLACEMENT_CHARACTER);
        dst.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
    };
    if src.first() != Some(&b'"') {
        return;
    }
    let (mut i, mut n) = (1, 1);
    while n < src.len() {
        n += src[n..]
            .iter()
            .take_while(|&&c| (b' '..0x80).contains(&c) && c != b'\\' && c != b'"')
            .count();
        let Some(&c) = src.get(n) else {
            break;
        };
        if c == b'"' {
            dst.extend_from_slice(&src[i..n]);
            return;
        }
        let (r, rn) = go_utf8::decode_rune(&src[n..]);
        if rn > 1 {
            n += rn;
            continue;
        }
        dst.extend_from_slice(&src[i..n]);
        if c == b'\\' {
            let Some(&e) = src.get(n + 1) else {
                return;
            };
            match e {
                b'"' | b'\\' | b'/' => dst.push(e),
                b'b' => dst.push(b'\x08'),
                b'f' => dst.push(b'\x0c'),
                b'n' => dst.push(b'\n'),
                b'r' => dst.push(b'\r'),
                b't' => dst.push(b'\t'),
                b'u' => {
                    let Some(v1) = src.get(n + 2..n + 6).and_then(parse_hex_u16) else {
                        return;
                    };
                    n += 6;
                    let mut r = v1;
                    if is_surrogate(v1) {
                        // A replacement unless a low surrogate escape follows.
                        r = go_utf8::RUNE_ERROR;
                        if let (Some(b"\\u"), Some(v2)) = (
                            src.get(n..n + 2),
                            src.get(n + 2..n + 6).and_then(parse_hex_u16),
                        ) && (0xD800..0xDC00).contains(&v1)
                            && (0xDC00..0xE000).contains(&v2)
                        {
                            r = 0x10000 + ((v1 - 0xD800) << 10) + (v2 - 0xDC00);
                            n += 6;
                        }
                    }
                    push_rune(dst, r);
                    i = n;
                    continue;
                }
                _ => return,
            }
            n += 2;
        } else if r == go_utf8::RUNE_ERROR {
            if !full_rune(&src[n..]) {
                return;
            }
            push_rune(dst, go_utf8::RUNE_ERROR);
            n += 1;
        } else {
            // A control character does not occur in a valid string.
            return;
        }
        i = n;
    }
    dst.extend_from_slice(&src[i..n.min(src.len())]);
}

/// The states of a number Go's `ConsumeNumberResumable` resumes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum NumberState {
    Init,
    BeforeIntegerDigits,
    WithinIntegerDigits,
    BeforeFractionalDigits,
    WithinFractionalDigits,
    BeforeExponentDigits,
    WithinExponentDigits,
}

/// Go's `ConsumeNumberResumable`: a JSON number from `b`, continuing at `resume` in `state`. A number is not
/// self-terminating: when it ends at the end of `b`, more input may continue it.
pub(crate) fn consume_number_resumable(
    b: &[u8],
    resume: usize,
    state: NumberState,
) -> (usize, NumberState, Option<ScanError>) {
    use NumberState::*;
    let digit = |i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    let digits_from = |mut i: usize| {
        while digit(i) {
            i += 1;
        }
        i
    };
    let expecting_digit =
        |i: usize| ScanError::invalid_character(&b[i..], "in number (expecting digit)");
    let mut n = resume;
    let mut state = state;
    // Jump to the part of the number to continue with.
    let mut part = 0;
    if state > Init {
        if matches!(
            state,
            WithinIntegerDigits | WithinFractionalDigits | WithinExponentDigits
        ) {
            n = digits_from(n);
            if n >= b.len() {
                return (n, state, None);
            }
            state = match state {
                WithinIntegerDigits => BeforeFractionalDigits,
                WithinFractionalDigits => BeforeExponentDigits,
                _ => return (n, state, None),
            };
        }
        part = match state {
            BeforeIntegerDigits => 0,
            BeforeFractionalDigits => 1,
            BeforeExponentDigits => 2,
            _ => return (n, state, None),
        };
    }

    // The required integer part, with an optional minus sign.
    if part == 0 {
        let resume = n;
        if b.first() == Some(&b'-') {
            n += 1;
        }
        match b.get(n) {
            None => return (resume, BeforeIntegerDigits, Some(ScanError::UnexpectedEof)),
            Some(b'0') => {
                n += 1;
                state = BeforeFractionalDigits;
            }
            Some(b'1'..=b'9') => {
                n = digits_from(n + 1);
                state = WithinIntegerDigits;
            }
            Some(_) => return (n, state, Some(expecting_digit(n))),
        }
    }

    // The optional fraction.
    if part <= 1 && b.get(n) == Some(&b'.') {
        let resume = n;
        n += 1;
        match b.get(n) {
            None => {
                return (
                    resume,
                    BeforeFractionalDigits,
                    Some(ScanError::UnexpectedEof),
                );
            }
            Some(c) if c.is_ascii_digit() => n = digits_from(n + 1),
            Some(_) => return (n, state, Some(expecting_digit(n))),
        }
        state = WithinFractionalDigits;
    }

    // The optional exponent.
    if matches!(b.get(n), Some(b'e' | b'E')) {
        let resume = n;
        n += 1;
        if matches!(b.get(n), Some(b'-' | b'+')) {
            n += 1;
        }
        match b.get(n) {
            None => return (resume, BeforeExponentDigits, Some(ScanError::UnexpectedEof)),
            Some(c) if c.is_ascii_digit() => n = digits_from(n + 1),
            Some(_) => return (n, state, Some(expecting_digit(n))),
        }
        state = WithinExponentDigits;
    }
    (n, state, None)
}
