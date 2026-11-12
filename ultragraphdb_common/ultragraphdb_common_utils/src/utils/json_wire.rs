/*
 * Copyright 2023 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/encoding/json/internal/jsonwire/decode.go and src/unicode/utf8/utf8.go (FullRune).
 * Changed for ultragraphdb: translated to Rust; strings are consumed as Go's v1 decoder consumes them, with
 * AllowInvalidUTF8, so neither invalid UTF-8 nor unpaired surrogate escapes are syntax errors.
 * Copyright (c) 2026. Marvin Hansen.
 */

// Arithmetic on decoded text input is checked: an overflow here would be a malformed input
// accepted with a wrapped value, so it is refused instead (user decision 2026-11-12; the lint is not in the
// workspace table because elsewhere it flags provably bounded index math).
#![deny(clippy::arithmetic_side_effects)]

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

/// `rest` past its leading whitespace.
pub(crate) fn skip_whitespace(mut rest: &[u8]) -> &[u8] {
    while let Some((&c, after)) = rest.split_first()
        && is_space(c)
    {
        rest = after;
    }
    rest
}

/// Go's `ConsumeLiteral`: `null`, `false` or `true`, or how far it matched.
pub(crate) fn consume_literal(b: &[u8], lit: &str) -> (usize, Option<ScanError>) {
    let l = lit.as_bytes();
    let mut rest = b;
    for &want in l {
        let Some((&c, after)) = rest.split_first() else {
            return (b.len(), Some(ScanError::UnexpectedEof));
        };
        if c != want {
            let expecting = crate::utils::go_strconv::quote_rune(char::from(want));
            let where_ = format!("in literal {lit} (expecting {expecting})");
            return (
                offset(b, rest),
                Some(ScanError::invalid_character(rest, where_)),
            );
        }
        rest = after;
    }
    (l.len(), None)
}

/// The offset in `b` of `rest`, which ends `b`: the difference of their lengths.
fn offset(b: &[u8], rest: &[u8]) -> usize {
    b.len().abs_diff(rest.len())
}

/// What `run` holds before `rest`, which ends it.
fn before<'a>(run: &'a [u8], rest: &[u8]) -> &'a [u8] {
    run.get(..offset(run, rest)).unwrap_or(run)
}

/// `rest` past its leading run of unescaped printable ASCII: no control character, quote or backslash.
fn skip_plain(mut rest: &[u8]) -> &[u8] {
    while let Some((&c, after)) = rest.split_first()
        && (b' '..0x80).contains(&c)
        && c != b'\\'
        && c != b'"'
    {
        rest = after;
    }
    rest
}

/// `rest` past its leading decimal digits.
fn skip_digits(mut rest: &[u8]) -> &[u8] {
    while let Some((c, after)) = rest.split_first()
        && c.is_ascii_digit()
    {
        rest = after;
    }
    rest
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
    if b.len() != 4 {
        return None;
    }
    b.iter()
        .try_fold(0, |v, &c| Some(v << 4 | char::from(c).to_digit(16)?))
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
    let start = match b.first() {
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
    let Some(mut rest) = b.get(start..) else {
        return (start, Some(ScanError::UnexpectedEof));
    };
    while !rest.is_empty() {
        // Long runs of unescaped ASCII.
        rest = skip_plain(rest);
        let n = offset(b, rest);
        let Some((&c, after)) = rest.split_first() else {
            return (n, Some(ScanError::UnexpectedEof));
        };
        if c == b'"' {
            return (offset(b, after), None);
        }
        let (r, rn) = go_utf8::decode_rune(rest);
        if rn > 1 {
            let Some(past_rune) = rest.get(rn..) else {
                return (n, Some(ScanError::UnexpectedEof));
            };
            rest = past_rune;
        } else if c == b'\\' {
            let Some((&e, after_escape)) = after.split_first() else {
                return (n, Some(ScanError::UnexpectedEof));
            };
            match e {
                b'/' | b'"' | b'\\' | b'b' | b'f' | b'n' | b'r' | b't' => rest = after_escape,
                b'u' => match consume_u_escape(rest) {
                    Ok(tail) => rest = tail,
                    Err(refused) => return (n, Some(refused)),
                },
                _ => {
                    return (
                        n,
                        Some(ScanError::invalid_escape(rest.get(..2).unwrap_or(rest))),
                    );
                }
            }
        } else if r == go_utf8::RUNE_ERROR {
            // A byte that is not UTF-8, unless more input may complete it.
            if !full_rune(rest) {
                return (n, Some(ScanError::UnexpectedEof));
            }
            rest = after;
        } else {
            // What is left of ASCII is a control character.
            return (
                n,
                Some(ScanError::invalid_character(
                    rest,
                    "in string (expecting non-control character)",
                )),
            );
        }
    }
    (offset(b, rest), Some(ScanError::UnexpectedEof))
}

/// The `\u` escape `rest` begins with, its four hexadecimal digits checked: what follows it.
fn consume_u_escape(rest: &[u8]) -> Result<&[u8], ScanError> {
    let Some((escape, tail)) = rest.split_at_checked(6) else {
        if has_escaped_utf16_prefix(rest, false) {
            return Err(ScanError::UnexpectedEof);
        }
        return Err(ScanError::invalid_escape(rest));
    };
    if escape.get(2..).and_then(parse_hex_u16).is_none() {
        return Err(ScanError::invalid_escape(escape));
    }
    Ok(tail)
}

/// Go's `utf16.IsSurrogate`.
fn is_surrogate(r: u32) -> bool {
    (0xD800..0xE000).contains(&r)
}

/// Go's `AppendUnquote` of a complete JSON string, its errors dropped as `UnquoteMayCopy` drops them: each byte
/// that is not UTF-8 and each unpaired surrogate escape becomes U+FFFD.
pub(crate) fn append_unquote(dst: &mut Vec<u8>, src: &[u8]) {
    let push_rune = |out: &mut Vec<u8>, r: u32| {
        let c = char::from_u32(r).unwrap_or(char::REPLACEMENT_CHARACTER);
        out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
    };
    let Some(mut rest) = src.strip_prefix(b"\"") else {
        return;
    };
    // The bytes since the last escape or replaced byte, copied as they are.
    let mut run = rest;
    while !rest.is_empty() {
        rest = skip_plain(rest);
        let Some((&c, after)) = rest.split_first() else {
            break;
        };
        if c == b'"' {
            dst.extend_from_slice(before(run, rest));
            return;
        }
        let (r, rn) = go_utf8::decode_rune(rest);
        if rn > 1 {
            let Some(past_rune) = rest.get(rn..) else {
                return;
            };
            rest = past_rune;
            continue;
        }
        dst.extend_from_slice(before(run, rest));
        if c == b'\\' {
            let Some((&e, after_escape)) = after.split_first() else {
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
                    let Some((rune, tail)) = unquote_u_escape(rest) else {
                        return;
                    };
                    push_rune(dst, rune);
                    rest = tail;
                    run = rest;
                    continue;
                }
                _ => return,
            }
            rest = after_escape;
        } else if r == go_utf8::RUNE_ERROR {
            if !full_rune(rest) {
                return;
            }
            push_rune(dst, go_utf8::RUNE_ERROR);
            rest = after;
        } else {
            // A control character does not occur in a valid string.
            return;
        }
        run = rest;
    }
    dst.extend_from_slice(before(run, rest));
}

/// The rune of the `\u` escape `rest` begins with and what follows it: a surrogate pair's rune when a low surrogate
/// escape follows a high one, U+FFFD for any other surrogate; `None` when its four hex digits are missing.
fn unquote_u_escape(rest: &[u8]) -> Option<(u32, &[u8])> {
    let (escape, mut tail) = rest.split_at_checked(6)?;
    let v1 = escape.get(2..).and_then(parse_hex_u16)?;
    let mut r = v1;
    if is_surrogate(v1) {
        // A replacement unless a low surrogate escape follows.
        r = go_utf8::RUNE_ERROR;
        if let Some((low, after)) = tail.split_at_checked(6)
            && low.starts_with(b"\\u")
            && let Some(v2) = low.get(2..).and_then(parse_hex_u16)
            && (0xD800..0xDC00).contains(&v1)
            && (0xDC00..0xE000).contains(&v2)
        {
            // Each half's low ten bits, the high half's above the low's: Go's `0x10000 + (r1-0xD800)<<10 + (r2-0xDC00)`.
            r = 0x10000_u32.checked_add((v1 & 0x3FF) << 10 | (v2 & 0x3FF))?;
            tail = after;
        }
    }
    Some((r, tail))
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
    // The offset past the byte at `i`, and past the digits from `i`; `i` itself where `b` ends before it.
    let past = |i: usize| {
        b.get(i..)
            .and_then(|rest| rest.get(1..))
            .map_or(i, |rest| offset(b, rest))
    };
    let digits_from = |i: usize| b.get(i..).map_or(i, |rest| offset(b, skip_digits(rest)));
    let expecting_digit = |i: usize| {
        ScanError::invalid_character(
            b.get(i..).unwrap_or_default(),
            "in number (expecting digit)",
        )
    };
    // Jump to the part of the number to continue with.
    let (mut n, mut state, part) = match resume_number(b, resume, state) {
        Ok(at) => at,
        Err((n, state)) => return (n, state, None),
    };

    // The required integer part, with an optional minus sign.
    if part == 0 {
        let start = n;
        if b.first() == Some(&b'-') {
            n = past(n);
        }
        match b.get(n) {
            None => return (start, BeforeIntegerDigits, Some(ScanError::UnexpectedEof)),
            Some(b'0') => {
                n = past(n);
                state = BeforeFractionalDigits;
            }
            Some(b'1'..=b'9') => {
                n = digits_from(past(n));
                state = WithinIntegerDigits;
            }
            Some(_) => return (n, state, Some(expecting_digit(n))),
        }
    }

    // The optional fraction.
    if part <= 1 && b.get(n) == Some(&b'.') {
        let start = n;
        n = past(n);
        match b.get(n) {
            None => {
                return (
                    start,
                    BeforeFractionalDigits,
                    Some(ScanError::UnexpectedEof),
                );
            }
            Some(c) if c.is_ascii_digit() => n = digits_from(past(n)),
            Some(_) => return (n, state, Some(expecting_digit(n))),
        }
        state = WithinFractionalDigits;
    }

    // The optional exponent.
    if matches!(b.get(n), Some(b'e' | b'E')) {
        let start = n;
        n = past(n);
        if matches!(b.get(n), Some(b'-' | b'+')) {
            n = past(n);
        }
        match b.get(n) {
            None => return (start, BeforeExponentDigits, Some(ScanError::UnexpectedEof)),
            Some(c) if c.is_ascii_digit() => n = digits_from(past(n)),
            Some(_) => return (n, state, Some(expecting_digit(n))),
        }
        state = WithinExponentDigits;
    }
    (n, state, None)
}

/// Where [`consume_number_resumable`] continues a number left at `n` in `state`: the offset, the state and the part
/// to go on with, 0 the integer, 1 the fraction, 2 the exponent; or, as the error, the offset and state the number
/// stops at without an error.
fn resume_number(
    b: &[u8],
    mut n: usize,
    mut state: NumberState,
) -> Result<(usize, NumberState, u8), (usize, NumberState)> {
    use NumberState::*;
    if state == Init {
        return Ok((n, state, 0));
    }
    if matches!(
        state,
        WithinIntegerDigits | WithinFractionalDigits | WithinExponentDigits
    ) {
        n = b.get(n..).map_or(n, |rest| offset(b, skip_digits(rest)));
        if n >= b.len() {
            return Err((n, state));
        }
        state = match state {
            WithinIntegerDigits => BeforeFractionalDigits,
            WithinFractionalDigits => BeforeExponentDigits,
            Init
            | BeforeIntegerDigits
            | BeforeFractionalDigits
            | BeforeExponentDigits
            | WithinExponentDigits => return Err((n, state)),
        };
    }
    match state {
        BeforeIntegerDigits => Ok((n, state, 0)),
        BeforeFractionalDigits => Ok((n, state, 1)),
        BeforeExponentDigits => Ok((n, state, 2)),
        Init | WithinIntegerDigits | WithinFractionalDigits | WithinExponentDigits => {
            Err((n, state))
        }
    }
}
