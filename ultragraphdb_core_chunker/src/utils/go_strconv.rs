/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26, src/internal/strconv/atoi.go, atof.go and src/strconv/quote.go. Changed for ultragraphdb:
 * translated to Rust; the subset Dgraph's chunker package calls. JSON numbers are converted by Rust's correctly
 * rounded parser, as their grammar is a subset of Go's.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Go's `strconv` as Dgraph's chunker uses it: `ParseUint(s, 0|10, 64)`, `ParseInt(s, 10, 64)`, `Atoi`,
//! `ParseFloat(s, 64)` of a JSON number, `Unquote` and `Quote`.

use crate::types::num_error::NumError;
use crate::utils::isprint::is_print;

fn lower(c: u8) -> u8 {
    c | (b'x' - b'X')
}

/// Go's `underscoreOK`: whether the underscores of a number literal only separate digits.
fn underscore_ok(s: &[u8]) -> bool {
    let mut s = s;
    let mut saw = b'^';
    let mut i = 0;
    if !s.is_empty() && (s[0] == b'-' || s[0] == b'+') {
        s = &s[1..];
    }
    let mut hex = false;
    if s.len() >= 2 && s[0] == b'0' && matches!(lower(s[1]), b'b' | b'o' | b'x') {
        i = 2;
        saw = b'0';
        hex = lower(s[1]) == b'x';
    }
    for &c in &s[i..] {
        if c.is_ascii_digit() || hex && (b'a'..=b'f').contains(&lower(c)) {
            saw = b'0';
            continue;
        }
        if c == b'_' {
            if saw != b'0' {
                return false;
            }
            saw = b'_';
            continue;
        }
        if saw == b'_' {
            return false;
        }
        saw = b'!';
    }
    saw != b'_'
}

/// Go's `strconv.ParseUint(s, base, 64)` for base 0 (prefixes `0b`, `0o`, `0x`, a leading `0` for octal,
/// underscores between digits) or 10.
pub(crate) fn parse_uint(s: &str, base: u32) -> Result<u64, NumError> {
    parse_uint_as("ParseUint", s, base)
}

fn parse_uint_as(func: &'static str, s: &str, base: u32) -> Result<u64, NumError> {
    let b = s.as_bytes();
    if b.is_empty() {
        return Err(NumError::syntax(func, s));
    }
    let base0 = base == 0;
    let (base, digits): (u64, &[u8]) = match (base0, b[0], b.get(1).map(|&c| lower(c))) {
        (false, _, _) => (u64::from(base), b),
        (true, b'0', Some(b'b')) if b.len() >= 3 => (2, &b[2..]),
        (true, b'0', Some(b'o')) if b.len() >= 3 => (8, &b[2..]),
        (true, b'0', Some(b'x')) if b.len() >= 3 => (16, &b[2..]),
        (true, b'0', _) => (8, &b[1..]),
        (true, _, _) => (10, b),
    };
    let cutoff = u64::MAX / base + 1;
    let mut underscores = false;
    let mut n: u64 = 0;
    for &c in digits {
        let d = match c {
            b'_' if base0 => {
                underscores = true;
                continue;
            }
            b'0'..=b'9' => c - b'0',
            _ if lower(c).is_ascii_lowercase() => lower(c) - b'a' + 10,
            _ => return Err(NumError::syntax(func, s)),
        };
        if u64::from(d) >= base {
            return Err(NumError::syntax(func, s));
        }
        if n >= cutoff {
            return Err(NumError::range(func, s));
        }
        n = (n * base)
            .checked_add(u64::from(d))
            .ok_or_else(|| NumError::range(func, s))?;
    }
    if underscores && !underscore_ok(b) {
        return Err(NumError::syntax(func, s));
    }
    Ok(n)
}

fn parse_int_as(func: &'static str, s: &str) -> Result<i64, NumError> {
    let (neg, rest) = match s.as_bytes().first() {
        None => return Err(NumError::syntax(func, s)),
        Some(b'+') => (false, &s[1..]),
        Some(b'-') => (true, &s[1..]),
        Some(_) => (false, s),
    };
    let un = match parse_uint_as(func, rest, 10) {
        Ok(un) => un,
        Err(e) if !e.range => return Err(NumError::syntax(func, s)),
        Err(_) => u64::MAX,
    };
    let cutoff = 1u64 << 63;
    if !neg && un >= cutoff || neg && un > cutoff {
        return Err(NumError::range(func, s));
    }
    Ok(if neg {
        0i64.wrapping_sub_unsigned(un)
    } else {
        un as i64
    })
}

/// Go's `strconv.ParseInt(s, 10, 64)`, which `json.Number.Int64` calls.
pub(crate) fn parse_int(s: &str) -> Result<i64, NumError> {
    parse_int_as("ParseInt", s)
}

/// Go's `strconv.Atoi` on a 64-bit platform.
pub(crate) fn atoi(s: &str) -> Result<i64, NumError> {
    parse_int_as("Atoi", s)
}

/// Go's `strconv.ParseFloat(s, 64)` of a JSON number, which `json.Number.Float64` calls.
pub(crate) fn parse_json_float(s: &str) -> Result<f64, NumError> {
    match s.parse::<f64>() {
        Ok(f) if f.is_infinite() => Err(NumError::range("ParseFloat", s)),
        Ok(f) => Ok(f),
        Err(_) => Err(NumError::syntax("ParseFloat", s)),
    }
}

fn unhex(c: u8) -> Option<u32> {
    char::from(c).to_digit(16)
}

/// Go's `strconv.UnquoteChar(s, '"')`: the bytes of the next character and the rest; `None` is `ErrSyntax`.
fn unquote_char(s: &str) -> Option<(Vec<u8>, &str)> {
    let mut chars = s.chars();
    let c = chars.next()?;
    if c == '"' {
        return None;
    }
    if c != '\\' {
        return Some((c.to_string().into_bytes(), chars.as_str()));
    }
    let e = *s.as_bytes().get(1).filter(|e| e.is_ascii())?;
    let rest = &s[2..];
    let simple = |v: u8| Some((vec![v], rest));
    match e {
        b'a' => simple(7),
        b'b' => simple(8),
        b'f' => simple(12),
        b'n' => simple(b'\n'),
        b'r' => simple(b'\r'),
        b't' => simple(b'\t'),
        b'v' => simple(11),
        b'\\' => simple(b'\\'),
        b'"' => simple(b'"'),
        b'x' | b'u' | b'U' => {
            let n = match e {
                b'x' => 2,
                b'u' => 4,
                _ => 8,
            };
            let digits = rest.as_bytes().get(..n)?;
            let v = digits
                .iter()
                .try_fold(0u32, |v, &d| Some(v << 4 | unhex(d)?))?;
            let tail = &rest[n..];
            if e == b'x' {
                // A single byte, possibly not UTF-8.
                return Some((vec![v as u8], tail));
            }
            Some((char::from_u32(v)?.to_string().into_bytes(), tail))
        }
        b'0'..=b'7' => {
            let digits = rest.as_bytes().get(..2)?;
            let v = digits.iter().try_fold(u32::from(e - b'0'), |v, &d| {
                (b'0'..=b'7')
                    .contains(&d)
                    .then(|| v << 3 | u32::from(d - b'0'))
            })?;
            u8::try_from(v).ok().map(|v| (vec![v], &rest[2..]))
        }
        _ => None,
    }
}

/// Go's `strconv.Unquote` of a double-quoted string, as bytes: `\x` and octal escapes may write bytes that are
/// not UTF-8. `None` is Go's `ErrSyntax`.
pub(crate) fn unquote(s: &str) -> Option<Vec<u8>> {
    let mut rest = s.strip_prefix('"')?;
    let mut buf = Vec::with_capacity(s.len());
    while !rest.is_empty() && !rest.starts_with('"') {
        if rest.starts_with('\n') {
            return None;
        }
        let (bytes, tail) = unquote_char(rest)?;
        buf.extend_from_slice(&bytes);
        rest = tail;
    }
    (rest == "\"").then_some(buf)
}

/// Go's `strconv.Quote`: the string in double quotes, with the runes Go's `IsPrint` rejects escaped.
pub(crate) fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for r in s.chars() {
        match r {
            '"' | '\\' => {
                out.push('\\');
                out.push(r);
            }
            _ if is_print(r) => out.push(r),
            '\x07' => out.push_str("\\a"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x0b' => out.push_str("\\v"),
            _ if (r as u32) < 0x20 || r as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", r as u32));
            }
            _ if (r as u32) < 0x10000 => out.push_str(&format!("\\u{:04x}", r as u32)),
            _ => out.push_str(&format!("\\U{:08x}", r as u32)),
        }
    }
    out.push('"');
    out
}
