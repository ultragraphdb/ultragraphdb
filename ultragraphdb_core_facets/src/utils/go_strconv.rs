/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26, src/internal/strconv/atoi.go, atof.go and src/strconv/quote.go. Changed for ultragraphdb:
 * translated to Rust; the subset Dgraph's facets package calls. Decimal floats are converted by Rust's correctly
 * rounded parser once Go's grammar has accepted them.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Go's `strconv` as Dgraph's facets use it: `ParseInt(s, 0, 64)`, `ParseFloat(s, 64)`, `Unquote` and `Quote`.

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

/// Go's `strconv.ParseUint(s, 0, 64)`: base prefixes `0b`, `0o`, `0x`, a leading `0` for octal, underscores
/// between digits.
fn parse_uint_base0(s: &str) -> Result<u64, bool> {
    // Err(true) is ErrRange, Err(false) ErrSyntax.
    let b = s.as_bytes();
    if b.is_empty() {
        return Err(false);
    }
    let (base, digits): (u64, &[u8]) = if b[0] == b'0' {
        match b.get(1).map(|&c| lower(c)) {
            Some(b'b') if b.len() >= 3 => (2, &b[2..]),
            Some(b'o') if b.len() >= 3 => (8, &b[2..]),
            Some(b'x') if b.len() >= 3 => (16, &b[2..]),
            _ => (8, &b[1..]),
        }
    } else {
        (10, b)
    };
    let cutoff = u64::MAX / base + 1;
    let mut underscores = false;
    let mut n: u64 = 0;
    for &c in digits {
        let d = match c {
            b'_' => {
                underscores = true;
                continue;
            }
            b'0'..=b'9' => c - b'0',
            _ if lower(c).is_ascii_lowercase() => lower(c) - b'a' + 10,
            _ => return Err(false),
        };
        if u64::from(d) >= base {
            return Err(false);
        }
        if n >= cutoff {
            return Err(true);
        }
        n *= base;
        n = n.checked_add(u64::from(d)).ok_or(true)?;
    }
    if underscores && !underscore_ok(b) {
        return Err(false);
    }
    Ok(n)
}

/// Go's `strconv.ParseInt(s, 0, 64)`.
pub(crate) fn parse_int_base0(s: &str) -> Result<i64, NumError> {
    const FN: &str = "ParseInt";
    let (neg, rest) = match s.as_bytes().first() {
        None => return Err(NumError::syntax(FN, s)),
        Some(b'+') => (false, &s[1..]),
        Some(b'-') => (true, &s[1..]),
        Some(_) => (false, s),
    };
    let un = match parse_uint_base0(rest) {
        Ok(un) => un,
        Err(false) => return Err(NumError::syntax(FN, s)),
        Err(true) => u64::MAX,
    };
    let cutoff = 1u64 << 63;
    if !neg && un >= cutoff || neg && un > cutoff {
        return Err(NumError::range(FN, s));
    }
    Ok(if neg {
        0i64.wrapping_sub_unsigned(un)
    } else {
        un as i64
    })
}

fn common_prefix_len_ignore_case(s: &[u8], prefix: &[u8]) -> usize {
    s.iter()
        .zip(prefix)
        .take_while(|(a, p)| lower(**a) == **p)
        .count()
}

/// Go's `special`: a signed infinity or an unsigned NaN, case-insensitive, and how many bytes it takes.
fn special(s: &[u8]) -> Option<(f64, usize)> {
    let (sign, nsign, rest) = match s.first()? {
        b'+' => (1.0, 1, &s[1..]),
        b'-' => (-1.0, 1, &s[1..]),
        _ => (1.0, 0, s),
    };
    match rest.first().map(|&c| lower(c)) {
        Some(b'i') => {
            let mut n = common_prefix_len_ignore_case(rest, b"infinity");
            if 3 < n && n < 8 {
                n = 3;
            }
            (n == 3 || n == 8).then_some((sign * f64::INFINITY, nsign + n))
        }
        Some(b'n') if nsign == 0 && common_prefix_len_ignore_case(rest, b"nan") == 3 => {
            Some((f64::NAN, 3))
        }
        _ => None,
    }
}

/// What Go's `readFloat` reads: the mantissa digits that fit, the binary or decimal exponent, the sign, whether
/// digits were dropped, whether the literal is hexadecimal, and how many bytes it takes.
struct ReadFloat {
    mantissa: u64,
    exp: i64,
    neg: bool,
    trunc: bool,
    hex: bool,
    n: usize,
}

/// Go's `readFloat`: the longest float literal at the start of `s`, `None` when there is none.
fn read_float(s: &[u8]) -> Option<ReadFloat> {
    let mut i = 0;
    let mut underscores = false;
    let mut neg = false;
    match s.first()? {
        b'+' => i += 1,
        b'-' => {
            i += 1;
            neg = true;
        }
        _ => {}
    }
    let mut base: u64 = 10;
    let mut max_mant_digits = 19;
    let mut exp_char = b'e';
    let mut hex = false;
    if i + 2 < s.len() && s[i] == b'0' && lower(s[i + 1]) == b'x' {
        base = 16;
        max_mant_digits = 16;
        i += 2;
        exp_char = b'p';
        hex = true;
    }
    let (mut sawdot, mut sawdigits) = (false, false);
    let (mut nd, mut nd_mant, mut dp) = (0i64, 0i64, 0i64);
    let mut mantissa: u64 = 0;
    let mut trunc = false;
    while i < s.len() {
        let c = s[i];
        if c == b'_' {
            underscores = true;
        } else if c == b'.' {
            if sawdot {
                break;
            }
            sawdot = true;
            dp = nd;
        } else if c.is_ascii_digit() {
            sawdigits = true;
            if c == b'0' && nd == 0 {
                dp -= 1;
            } else {
                nd += 1;
                if nd_mant < max_mant_digits {
                    mantissa = mantissa * base + u64::from(c - b'0');
                    nd_mant += 1;
                } else if c != b'0' {
                    trunc = true;
                }
            }
        } else if base == 16 && (b'a'..=b'f').contains(&lower(c)) {
            sawdigits = true;
            nd += 1;
            if nd_mant < max_mant_digits {
                mantissa = mantissa * 16 + u64::from(lower(c) - b'a' + 10);
                nd_mant += 1;
            } else {
                trunc = true;
            }
        } else {
            break;
        }
        i += 1;
    }
    if !sawdigits {
        return None;
    }
    if !sawdot {
        dp = nd;
    }
    if base == 16 {
        dp *= 4;
        nd_mant *= 4;
    }
    if i < s.len() && lower(s[i]) == exp_char {
        i += 1;
        let mut esign = 1;
        match s.get(i)? {
            b'+' => i += 1,
            b'-' => {
                i += 1;
                esign = -1;
            }
            _ => {}
        }
        if !s.get(i)?.is_ascii_digit() {
            return None;
        }
        let mut e = 0i64;
        while i < s.len() && (s[i].is_ascii_digit() || s[i] == b'_') {
            if s[i] == b'_' {
                underscores = true;
            } else if e < 10000 {
                e = e * 10 + i64::from(s[i] - b'0');
            }
            i += 1;
        }
        dp += e * esign;
    } else if base == 16 {
        return None;
    }
    let exp = if mantissa != 0 { dp - nd_mant } else { 0 };
    if underscores && !underscore_ok(&s[..i]) {
        return None;
    }
    Some(ReadFloat {
        mantissa,
        exp,
        neg,
        trunc,
        hex,
        n: i,
    })
}

/// Go's `atofHex` for float64: the mantissa times 2^exp, rounded to nearest even; `Err` carries the infinity a
/// range error returns.
fn atof_hex(r: &ReadFloat) -> Result<f64, f64> {
    const MANTBITS: u32 = 52;
    const EXPBITS: u32 = 11;
    const BIAS: i64 = -1023;
    let max_exp = (1i64 << EXPBITS) + BIAS - 2;
    let min_exp = BIAS + 1;
    let mut mantissa = r.mantissa;
    let mut exp = r.exp + i64::from(MANTBITS);
    while mantissa != 0 && mantissa >> (MANTBITS + 2) == 0 {
        mantissa <<= 1;
        exp -= 1;
    }
    if r.trunc {
        mantissa |= 1;
    }
    while mantissa >> (1 + MANTBITS + 2) != 0 {
        mantissa = mantissa >> 1 | mantissa & 1;
        exp += 1;
    }
    while mantissa > 1 && exp < min_exp - 2 {
        mantissa = mantissa >> 1 | mantissa & 1;
        exp += 1;
    }
    let mut round = mantissa & 3;
    mantissa >>= 2;
    round |= mantissa & 1;
    exp += 2;
    if round == 3 {
        mantissa += 1;
        if mantissa == 1 << (1 + MANTBITS) {
            mantissa >>= 1;
            exp += 1;
        }
    }
    if mantissa >> MANTBITS == 0 {
        exp = BIAS;
    }
    let overflow = exp > max_exp;
    if overflow {
        mantissa = 1 << MANTBITS;
        exp = max_exp + 1;
    }
    let mut bits = mantissa & ((1 << MANTBITS) - 1);
    bits |= (((exp - BIAS) & ((1 << EXPBITS) - 1)) as u64) << MANTBITS;
    if r.neg {
        bits |= 1 << MANTBITS << EXPBITS;
    }
    let f = f64::from_bits(bits);
    if overflow { Err(f) } else { Ok(f) }
}

/// Go's `strconv.ParseFloat(s, 64)`: decimal and hexadecimal literals with underscores between digits,
/// infinities and NaN.
pub(crate) fn parse_float(s: &str) -> Result<f64, NumError> {
    const FN: &str = "ParseFloat";
    let b = s.as_bytes();
    if let Some((f, n)) = special(b) {
        return if n == b.len() {
            Ok(f)
        } else {
            Err(NumError::syntax(FN, s))
        };
    }
    let r = match read_float(b) {
        Some(r) if r.n == b.len() => r,
        _ => return Err(NumError::syntax(FN, s)),
    };
    let f = if r.hex {
        atof_hex(&r).map_err(|_| NumError::range(FN, s))?
    } else {
        // Go's grammar accepted the literal; without its underscores it is one Rust's parser takes.
        s.replace('_', "")
            .parse::<f64>()
            .map_err(|_| NumError::syntax(FN, s))?
    };
    if f.is_infinite() {
        return Err(NumError::range(FN, s));
    }
    Ok(f)
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
