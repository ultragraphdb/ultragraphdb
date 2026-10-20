/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26, src/internal/strconv/atoi.go, atof.go, atob.go, ftoa.go and src/strconv/quote.go.
 * Changed for ultragraphdb: translated to Rust; the subset Dgraph's dql package calls. Decimal floats are
 * converted by Rust's correctly rounded parser once Go's grammar has accepted them.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::num_error::NumError;

fn syntax_error(func: &'static str, s: &str) -> NumError {
    NumError {
        func,
        num: s.to_string(),
        range: false,
    }
}

fn range_error(func: &'static str, s: &str) -> NumError {
    NumError {
        func,
        num: s.to_string(),
        range: true,
    }
}

fn lower(c: u8) -> u8 {
    c | (b'x' - b'X')
}

/// Go's `underscoreOK`: whether the underscores of a number literal only separate digits.
fn underscore_ok(s: &str) -> bool {
    let mut s = s.as_bytes();
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
    while i < s.len() {
        let c = s[i];
        i += 1;
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

/// Go's `strconv.ParseUint(s, base, 64)` for base 0 (prefixes and underscores) or 10.
pub(crate) fn parse_uint(s: &str, base: u32) -> Result<u64, NumError> {
    const FN: &str = "ParseUint";
    if s.is_empty() {
        return Err(syntax_error(FN, s));
    }
    let base0 = base == 0;
    let b = s.as_bytes();
    let (base, digits): (u64, &[u8]) = if base0 {
        if b[0] == b'0' {
            match b.get(1).map(|&c| lower(c)) {
                Some(b'b') if b.len() >= 3 => (2, &b[2..]),
                Some(b'o') if b.len() >= 3 => (8, &b[2..]),
                Some(b'x') if b.len() >= 3 => (16, &b[2..]),
                _ => (8, &b[1..]),
            }
        } else {
            (10, b)
        }
    } else {
        (u64::from(base), b)
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
            _ => return Err(syntax_error(FN, s)),
        };
        if u64::from(d) >= base {
            return Err(syntax_error(FN, s));
        }
        if n >= cutoff {
            return Err(range_error(FN, s));
        }
        n *= base;
        match n.checked_add(u64::from(d)) {
            Some(n1) => n = n1,
            None => return Err(range_error(FN, s)),
        }
    }
    if underscores && !underscore_ok(s) {
        return Err(syntax_error(FN, s));
    }
    Ok(n)
}

/// Go's `strconv.ParseInt(s, base, 64)` for base 0 or 10.
pub(crate) fn parse_int(s: &str, base: u32) -> Result<i64, NumError> {
    const FN: &str = "ParseInt";
    if s.is_empty() {
        return Err(syntax_error(FN, s));
    }
    let (neg, rest) = match s.as_bytes()[0] {
        b'+' => (false, &s[1..]),
        b'-' => (true, &s[1..]),
        _ => (false, s),
    };
    let un = match parse_uint(rest, base) {
        Ok(un) => un,
        Err(e) if !e.range => return Err(syntax_error(FN, s)),
        Err(_) => u64::MAX,
    };
    let cutoff = 1u64 << 63;
    if !neg && un >= cutoff || neg && un > cutoff {
        return Err(range_error(FN, s));
    }
    Ok(if neg {
        (un as i64).wrapping_neg()
    } else {
        un as i64
    })
}

/// Go's `strconv.ParseBool`.
pub(crate) fn parse_bool(s: &str) -> Result<bool, NumError> {
    match s {
        "1" | "t" | "T" | "TRUE" | "true" | "True" => Ok(true),
        "0" | "f" | "F" | "FALSE" | "false" | "False" => Ok(false),
        _ => Err(syntax_error("ParseBool", s)),
    }
}

/// Go's `special`: infinities and NaN, case-insensitive, with the length they take.
fn special(s: &str) -> Option<(f64, usize)> {
    let b = s.as_bytes();
    let prefix = |s: &[u8], word: &[u8]| {
        s.iter()
            .zip(word)
            .take_while(|(a, w)| lower(**a) == **w)
            .count()
    };
    let (sign, nsign, rest) = match b.first()? {
        b'+' => (1.0, 1, &b[1..]),
        b'-' => (-1.0, 1, &b[1..]),
        _ => (1.0, 0, b),
    };
    match rest.first().map(|&c| lower(c)) {
        Some(b'i') => {
            let mut n = prefix(rest, b"infinity");
            if 3 < n && n < 8 {
                n = 3;
            }
            (n == 3 || n == 8).then_some((sign * f64::INFINITY, nsign + n))
        }
        Some(b'n') if nsign == 0 && prefix(rest, b"nan") == 3 => Some((f64::NAN, 3)),
        _ => None,
    }
}

/// Go's `readFloat` grammar: how many bytes form a float literal, whether it is hexadecimal, and `None` when
/// none does.
fn read_float(s: &[u8]) -> Option<(usize, bool)> {
    let mut i = 0;
    let mut underscores = false;
    if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
        i += 1;
    }
    let mut hex = false;
    let exp_char = if i + 2 < s.len() && s[i] == b'0' && lower(s[i + 1]) == b'x' {
        i += 2;
        hex = true;
        b'p'
    } else {
        b'e'
    };
    let mut sawdot = false;
    let mut sawdigits = false;
    while i < s.len() {
        let c = s[i];
        if c == b'_' {
            underscores = true;
        } else if c == b'.' {
            if sawdot {
                break;
            }
            sawdot = true;
        } else if c.is_ascii_digit() || hex && (b'a'..=b'f').contains(&lower(c)) {
            sawdigits = true;
        } else {
            break;
        }
        i += 1;
    }
    if !sawdigits {
        return None;
    }
    if i < s.len() && lower(s[i]) == exp_char {
        i += 1;
        if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
            i += 1;
        }
        if i >= s.len() || !s[i].is_ascii_digit() {
            return None;
        }
        while i < s.len() && (s[i].is_ascii_digit() || s[i] == b'_') {
            underscores |= s[i] == b'_';
            i += 1;
        }
    } else if hex {
        return None;
    }
    if underscores && !underscore_ok(std::str::from_utf8(&s[..i]).ok()?) {
        return None;
    }
    Some((i, hex))
}

/// A hexadecimal float literal `[+-]0x<mantissa>p<exp>`, rounded through f64 arithmetic.
fn hex_float(s: &str) -> f64 {
    let s = s.replace('_', "");
    let (neg, body) = match s.as_bytes()[0] {
        b'-' => (true, &s[3..]),
        b'+' => (false, &s[3..]),
        _ => (false, &s[2..]),
    };
    let (mant, exp) = body.split_once(['p', 'P']).unwrap_or((body, "0"));
    let (int, frac) = mant.split_once('.').unwrap_or((mant, ""));
    let mut v = 0f64;
    for c in int.chars().chain(frac.chars()) {
        v = v * 16.0 + f64::from(c.to_digit(16).unwrap_or(0));
    }
    let exp: i32 = exp.parse().unwrap_or(0);
    let v = v * 2f64.powi(exp - 4 * frac.len() as i32);
    if neg { -v } else { v }
}

/// Go's `strconv.ParseFloat(s, 64)`.
pub(crate) fn parse_float(s: &str) -> Result<f64, NumError> {
    const FN: &str = "ParseFloat";
    if let Some((f, n)) = special(s) {
        return if n == s.len() {
            Ok(f)
        } else {
            Err(syntax_error(FN, s))
        };
    }
    let Some((n, hex)) = read_float(s.as_bytes()) else {
        return Err(syntax_error(FN, s));
    };
    if n != s.len() {
        return Err(syntax_error(FN, s));
    }
    let f = if hex {
        hex_float(s)
    } else {
        s.replace('_', "")
            .parse::<f64>()
            .map_err(|_| syntax_error(FN, s))?
    };
    if f.is_infinite() {
        return Err(range_error(FN, s));
    }
    Ok(f)
}

/// Go's `strconv.FormatFloat(f, 'E', -1, 64)`: the shortest digits, `d.dddE±XX`.
pub(crate) fn format_float_upper_e(f: f64) -> String {
    if f.is_nan() {
        return "NaN".to_string();
    }
    if f.is_infinite() {
        return if f > 0.0 { "+Inf" } else { "-Inf" }.to_string();
    }
    let s = format!("{f:E}");
    let (mant, exp) = s.split_once('E').unwrap_or((&s, "0"));
    let (sign, digits) = match exp.strip_prefix('-') {
        Some(d) => ('-', d),
        None => ('+', exp),
    };
    format!("{mant}E{sign}{digits:0>2}")
}

fn unhex(c: u8) -> Option<u32> {
    char::from(c).to_digit(16)
}

/// Go's `strconv.UnquoteChar` for a double-quoted string: the decoded bytes of the next character and the rest.
fn unquote_char(s: &[u8]) -> Option<(Vec<u8>, &[u8])> {
    let c = *s.first()?;
    if c == b'"' {
        return None;
    }
    if c >= 0x80 {
        let width = match c {
            0xF0.. => 4,
            0xE0.. => 3,
            _ => 2,
        };
        return Some((s[..width].to_vec(), &s[width..]));
    }
    if c != b'\\' {
        return Some((vec![c], &s[1..]));
    }
    let c = *s.get(1)?;
    let s = &s[2..];
    let simple = |b: u8| Some((vec![b], s));
    match c {
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
            let n = match c {
                b'x' => 2,
                b'u' => 4,
                _ => 8,
            };
            if s.len() < n {
                return None;
            }
            let v = s[..n]
                .iter()
                .try_fold(0u32, |v, &d| Some(v << 4 | unhex(d)?))?;
            let rest = &s[n..];
            if c == b'x' {
                return Some((vec![v as u8], rest));
            }
            let r = char::from_u32(v)?;
            Some((r.to_string().into_bytes(), rest))
        }
        b'0'..=b'7' => {
            if s.len() < 2 {
                return None;
            }
            let mut v = u32::from(c - b'0');
            for &d in &s[..2] {
                if !(b'0'..=b'7').contains(&d) {
                    return None;
                }
                v = v << 3 | u32::from(d - b'0');
            }
            if v > 255 {
                return None;
            }
            Some((vec![v as u8], &s[2..]))
        }
        _ => None,
    }
}

/// Go's `strconv.Unquote` of a double-quoted string; `None` is Go's `ErrSyntax`. Bytes that are not UTF-8
/// (from `\x` or octal escapes) are replaced, as a Rust string cannot hold them.
pub(crate) fn unquote(s: &str) -> Option<String> {
    let b = s.as_bytes();
    if b.len() < 2 || b[0] != b'"' {
        return None;
    }
    let mut buf = Vec::with_capacity(b.len());
    let mut rest = &b[1..];
    while !rest.is_empty() && rest[0] != b'"' {
        if rest[0] == b'\n' {
            return None;
        }
        let (bytes, tail) = unquote_char(rest)?;
        buf.extend_from_slice(&bytes);
        rest = tail;
    }
    if rest != b"\"" {
        return None;
    }
    Some(String::from_utf8_lossy(&buf).into_owned())
}
