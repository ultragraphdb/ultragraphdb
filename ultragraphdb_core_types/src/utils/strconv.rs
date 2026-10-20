/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26, src/internal/strconv/atof.go, atoi.go, atob.go, ftoa.go and src/strconv/quote.go.
 * Changed for ultragraphdb: translated to Rust; the subset Dgraph's types package calls. Shortest digits come
 * from Rust's float formatting, which finds the same shortest round-trip digits as Go's Ryū.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Go's `strconv` as Dgraph's scalar types use it: `ParseInt(s, 10, 64)`, `ParseFloat(s, 64|32)`, `ParseBool`,
//! `FormatFloat(f, 'G'|'g'|'e'|'f', -1, 64|32)`, `Quote`, and `encoding/json`'s float layout.

use dashu_int::UBig;

use crate::errors::types_error::TypesError;
use crate::utils::isprint::is_print;

fn num_error(func: &str, s: &[u8], range: bool) -> TypesError {
    let why = if range {
        "value out of range"
    } else {
        "invalid syntax"
    };
    TypesError::number(format!("strconv.{func}: parsing {}: {why}", quote(s)))
}

/// Go's `strconv.ParseInt(s, 10, 64)`.
pub(crate) fn parse_int(s: &[u8]) -> Result<i64, TypesError> {
    let b = s;
    if b.is_empty() {
        return Err(num_error("ParseInt", s, false));
    }
    let (neg, digits) = match b[0] {
        b'+' => (false, &b[1..]),
        b'-' => (true, &b[1..]),
        _ => (false, b),
    };
    if digits.is_empty() {
        return Err(num_error("ParseInt", s, false));
    }
    let mut n: u64 = 0;
    let mut overflow = false;
    for &c in digits {
        if !c.is_ascii_digit() {
            return Err(num_error("ParseInt", s, false));
        }
        if !overflow {
            match n
                .checked_mul(10)
                .and_then(|v| v.checked_add(u64::from(c - b'0')))
            {
                Some(v) => n = v,
                None => overflow = true,
            }
        }
    }
    let cutoff: u64 = 1 << 63;
    if overflow || (!neg && n >= cutoff) || (neg && n > cutoff) {
        return Err(num_error("ParseInt", s, true));
    }
    Ok(if neg {
        (n as i64).wrapping_neg()
    } else {
        n as i64
    })
}

/// Go's `strconv.ParseBool`.
pub(crate) fn parse_bool(s: &[u8]) -> Result<bool, TypesError> {
    match s {
        b"1" | b"t" | b"T" | b"TRUE" | b"true" | b"True" => Ok(true),
        b"0" | b"f" | b"F" | b"FALSE" | b"false" | b"False" => Ok(false),
        _ => Err(num_error("ParseBool", s, false)),
    }
}

/// Go's `strconv.FormatBool`.
pub(crate) fn format_bool(b: bool) -> &'static str {
    if b { "true" } else { "false" }
}

/// Go's `strconv.ParseFloat(s, 64)`.
pub(crate) fn parse_float64(s: &[u8]) -> Result<f64, TypesError> {
    parse_float(s, 64)
}

/// Go's `strconv.ParseFloat(s, 32)`: the nearest float32, widened.
pub(crate) fn parse_float32(s: &[u8]) -> Result<f32, TypesError> {
    parse_float(s, 32).map(|f| f as f32)
}

fn parse_float(s: &[u8], bits: u32) -> Result<f64, TypesError> {
    let (val, n, range) = match special(s) {
        Some((v, n)) => (v, n, false),
        None => match read_float(s) {
            None => return Err(num_error("ParseFloat", s, false)),
            Some(r) => {
                let (v, range) = if r.hex {
                    atof_hex(r.mantissa, r.exp, r.neg, r.trunc, bits)
                } else {
                    atof_decimal(&s[..r.n], bits)
                };
                (v, r.n, range)
            }
        },
    };
    if n != s.len() {
        return Err(num_error("ParseFloat", s, false));
    }
    if range {
        return Err(num_error("ParseFloat", s, true));
    }
    Ok(val)
}

fn common_prefix_len_ignore_case(s: &[u8], prefix: &[u8]) -> usize {
    s.iter()
        .zip(prefix)
        .take_while(|(a, b)| a.to_ascii_lowercase() == **b)
        .count()
}

/// Go's `special`: NaN, Inf and Infinity, signed, any case; the value and the bytes consumed.
fn special(s: &[u8]) -> Option<(f64, usize)> {
    let first = *s.first()?;
    let (sign, nsign, rest) = match first {
        b'+' => (1.0, 1, &s[1..]),
        b'-' => (-1.0, 1, &s[1..]),
        b'n' | b'N' => {
            return (common_prefix_len_ignore_case(s, b"nan") == 3).then_some((f64::NAN, 3));
        }
        b'i' | b'I' => (1.0, 0, s),
        _ => return None,
    };
    let mut n = common_prefix_len_ignore_case(rest, b"infinity");
    if 3 < n && n < 8 {
        n = 3;
    }
    (n == 3 || n == 8).then_some((sign * f64::INFINITY, nsign + n))
}

struct ReadFloat {
    mantissa: u64,
    exp: i64,
    neg: bool,
    trunc: bool,
    hex: bool,
    n: usize,
}

/// Go's `readFloat`: the syntax of a decimal or hexadecimal float, its leading mantissa digits and exponent.
fn read_float(s: &[u8]) -> Option<ReadFloat> {
    let mut underscores = false;
    let mut i = 0;
    let mut neg = false;
    if i >= s.len() {
        return None;
    }
    match s[i] {
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
    if i + 2 < s.len() && s[i] == b'0' && s[i + 1].eq_ignore_ascii_case(&b'x') {
        base = 16;
        max_mant_digits = 16;
        i += 2;
        exp_char = b'p';
        hex = true;
    }
    let mut sawdot = false;
    let mut sawdigits = false;
    let mut nd: i64 = 0;
    let mut nd_mant: i64 = 0;
    let mut dp: i64 = 0;
    let mut mantissa: u64 = 0;
    let mut trunc = false;
    while i < s.len() {
        let c = s[i];
        let lc = c.to_ascii_lowercase();
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
                i += 1;
                continue;
            }
            nd += 1;
            if nd_mant < max_mant_digits {
                mantissa = mantissa
                    .wrapping_mul(base)
                    .wrapping_add(u64::from(c - b'0'));
                nd_mant += 1;
            } else if c != b'0' {
                trunc = true;
            }
        } else if base == 16 && (b'a'..=b'f').contains(&lc) {
            sawdigits = true;
            nd += 1;
            if nd_mant < max_mant_digits {
                mantissa = mantissa
                    .wrapping_mul(16)
                    .wrapping_add(u64::from(lc - b'a' + 10));
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
    if i < s.len() && s[i].to_ascii_lowercase() == exp_char {
        i += 1;
        if i >= s.len() {
            return None;
        }
        let mut esign = 1;
        match s[i] {
            b'+' => i += 1,
            b'-' => {
                i += 1;
                esign = -1;
            }
            _ => {}
        }
        if i >= s.len() || !s[i].is_ascii_digit() {
            return None;
        }
        let mut e: i64 = 0;
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

/// Go's `underscoreOK`: underscores only between digits, or between a base prefix and a digit.
fn underscore_ok(s: &[u8]) -> bool {
    let mut saw = b'^';
    let mut i = 0;
    let s = if !s.is_empty() && (s[0] == b'-' || s[0] == b'+') {
        &s[1..]
    } else {
        s
    };
    let mut hex = false;
    if s.len() >= 2 && s[0] == b'0' && matches!(s[1].to_ascii_lowercase(), b'b' | b'o' | b'x') {
        i = 2;
        saw = b'0';
        hex = s[1].eq_ignore_ascii_case(&b'x');
    }
    while i < s.len() {
        let c = s[i];
        if c.is_ascii_digit() || hex && (b'a'..=b'f').contains(&c.to_ascii_lowercase()) {
            saw = b'0';
        } else if c == b'_' {
            if saw != b'0' {
                return false;
            }
            saw = b'_';
        } else {
            if saw == b'_' {
                return false;
            }
            saw = b'!';
        }
        i += 1;
    }
    saw != b'_'
}

/// A decimal float: Rust's parse is correctly rounded, as Go's Eisel-Lemire with its exact fallback is.
fn atof_decimal(s: &[u8], bits: u32) -> (f64, bool) {
    let clean: String = s
        .iter()
        .filter(|&&c| c != b'_')
        .map(|&c| char::from(c))
        .collect();
    if bits == 32 {
        let f: f32 = clean.parse().unwrap_or(0.0);
        (f64::from(f), f.is_infinite())
    } else {
        let f: f64 = clean.parse().unwrap_or(0.0);
        (f, f.is_infinite())
    }
}

/// Go's `atofHex`: rounds a hexadecimal mantissa and binary exponent to the float's bits, half to even.
fn atof_hex(mut mantissa: u64, exp: i64, neg: bool, trunc: bool, bits: u32) -> (f64, bool) {
    let (mantbits, expbits, bias): (u32, u32, i64) = if bits == 32 {
        (23, 8, -127)
    } else {
        (52, 11, -1023)
    };
    let max_exp = (1i64 << expbits) + bias - 2;
    let min_exp = bias + 1;
    let mut exp = exp + i64::from(mantbits);
    while mantissa != 0 && mantissa >> (mantbits + 2) == 0 {
        mantissa <<= 1;
        exp -= 1;
    }
    if trunc {
        mantissa |= 1;
    }
    while mantissa >> (1 + mantbits + 2) != 0 {
        mantissa = (mantissa >> 1) | (mantissa & 1);
        exp += 1;
    }
    while mantissa > 1 && exp < min_exp - 2 {
        mantissa = (mantissa >> 1) | (mantissa & 1);
        exp += 1;
    }
    let mut round = mantissa & 3;
    mantissa >>= 2;
    round |= mantissa & 1;
    exp += 2;
    if round == 3 {
        mantissa += 1;
        if mantissa == 1 << (1 + mantbits) {
            mantissa >>= 1;
            exp += 1;
        }
    }
    if mantissa >> mantbits == 0 {
        exp = bias;
    }
    let mut range = false;
    if exp > max_exp {
        mantissa = 1 << mantbits;
        exp = max_exp + 1;
        range = true;
    }
    let mut b = mantissa & ((1u64 << mantbits) - 1);
    b |= (((exp - bias) as u64) & ((1u64 << expbits) - 1)) << mantbits;
    if neg {
        b |= 1u64 << mantbits << expbits;
    }
    if bits == 32 {
        (f64::from(f32::from_bits(b as u32)), range)
    } else {
        (f64::from_bits(b), range)
    }
}

/// The shortest decimal digits of a finite, nonzero float and the position of the decimal point, Go's
/// `decimalSlice` (`d` and `dp`: the value is 0.d × 10^dp).
///
/// Rust's shortest search and Go's agree except where two shortest candidates lie exactly as far from the value;
/// Go takes the even one (its digits are the exact value rounded half to even), so that case is redone here.
fn shortest_digits(f: f64, bits: u32) -> (Vec<u8>, i64) {
    let sci = if bits == 32 {
        format!("{:e}", (f as f32).abs())
    } else {
        format!("{:e}", f.abs())
    };
    let (mant, exp) = sci.split_once('e').expect("Rust's {:e} has an exponent");
    let exp: i64 = exp.parse().expect("Rust's {:e} exponent is an integer");
    let digits: Vec<u8> = mant.bytes().filter(u8::is_ascii_digit).collect();
    let dp = exp + 1;
    let v = if bits == 32 {
        f64::from(f as f32).abs()
    } else {
        f.abs()
    };
    let (exact, edp) = exact_digits(v);
    if exact.len() > digits.len() {
        let (r, rdp) = round_half_even(&exact, edp, digits.len());
        if (r.as_slice(), rdp) != (digits.as_slice(), dp) && round_trips(&r, rdp, v, bits) {
            return (r, rdp);
        }
    }
    (digits, dp)
}

/// The exact decimal digits of a positive finite float and their decimal point.
fn exact_digits(v: f64) -> (Vec<u8>, i64) {
    let b = v.to_bits();
    let biased = ((b >> 52) & 0x7ff) as i64;
    let frac = b & ((1 << 52) - 1);
    let (m, e) = if biased == 0 {
        (frac, -1074)
    } else {
        (frac | (1 << 52), biased - 1075)
    };
    let m = UBig::from(m);
    let (n, exp10) = if e >= 0 {
        (m << e as usize, 0)
    } else {
        (m * UBig::from(5u8).pow((-e) as usize), e)
    };
    let s = n.to_string();
    let t = s.trim_end_matches('0');
    (t.as_bytes().to_vec(), s.len() as i64 + exp10)
}

/// Digits rounded half to even to `n` digits, trailing zeros trimmed.
fn round_half_even(d: &[u8], dp: i64, n: usize) -> (Vec<u8>, i64) {
    let rest_zero = d[n + 1..].iter().all(|&c| c == b'0');
    let up = d[n] > b'5' || d[n] == b'5' && (!rest_zero || (n > 0 && (d[n - 1] - b'0') % 2 == 1));
    let mut r = d[..n].to_vec();
    let mut dp = dp;
    if up {
        let mut i = n;
        loop {
            if i == 0 {
                r.insert(0, b'1');
                r.pop();
                dp += 1;
                break;
            }
            i -= 1;
            if r[i] == b'9' {
                r[i] = b'0';
            } else {
                r[i] += 1;
                break;
            }
        }
    }
    while r.last() == Some(&b'0') {
        r.pop();
    }
    (r, dp)
}

fn round_trips(d: &[u8], dp: i64, v: f64, bits: u32) -> bool {
    let text = format!("0.{}e{dp}", std::str::from_utf8(d).expect("digits"));
    if bits == 32 {
        text.parse::<f32>().is_ok_and(|x| f64::from(x) == v)
    } else {
        text.parse::<f64>().is_ok_and(|x| x == v)
    }
}

/// Go's `strconv.FormatFloat(f, fmt, -1, bits)` for `fmt` one of `e`, `E`, `f`, `g`, `G`.
pub(crate) fn format_float(f: f64, fmt: u8, bits: u32) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f > 0.0 {
            "+Inf".into()
        } else {
            "-Inf".into()
        };
    }
    let neg = f.is_sign_negative();
    let (d, dp) = if f == 0.0 {
        (Vec::new(), 0)
    } else {
        shortest_digits(f, bits)
    };
    let nd = d.len() as i64;
    let mut out = String::new();
    match fmt {
        b'e' | b'E' => fmt_e(&mut out, neg, &d, dp, nd - 1, fmt),
        b'f' => fmt_f(&mut out, neg, &d, dp, (nd - dp).max(0)),
        _ => {
            let prec = nd;
            let exp = dp - 1;
            if !(-4..6).contains(&exp) {
                fmt_e(&mut out, neg, &d, dp, prec - 1, fmt + b'e' - b'g');
            } else {
                fmt_f(&mut out, neg, &d, dp, (prec - dp).max(0));
            }
        }
    }
    out
}

fn fmt_e(out: &mut String, neg: bool, d: &[u8], dp: i64, prec: i64, fmt: u8) {
    if neg {
        out.push('-');
    }
    out.push(if d.is_empty() { '0' } else { d[0] as char });
    if prec > 0 {
        out.push('.');
        let mut i = 1;
        let m = d.len().min(prec as usize + 1);
        if i < m {
            d[i..m].iter().for_each(|&c| out.push(c as char));
            i = m;
        }
        while i as i64 <= prec {
            out.push('0');
            i += 1;
        }
    }
    out.push(fmt as char);
    let mut exp = if d.is_empty() { 0 } else { dp - 1 };
    if exp < 0 {
        out.push('-');
        exp = -exp;
    } else {
        out.push('+');
    }
    if exp < 10 {
        out.push('0');
    }
    out.push_str(&exp.to_string());
}

fn fmt_f(out: &mut String, neg: bool, d: &[u8], dp: i64, prec: i64) {
    if neg {
        out.push('-');
    }
    if dp > 0 {
        let m = d.len().min(dp as usize);
        d[..m].iter().for_each(|&c| out.push(c as char));
        (m..dp as usize).for_each(|_| out.push('0'));
    } else {
        out.push('0');
    }
    if prec > 0 {
        out.push('.');
        for i in 0..prec {
            let j = dp + i;
            out.push(if j >= 0 && (j as usize) < d.len() {
                d[j as usize] as char
            } else {
                '0'
            });
        }
    }
}

/// `encoding/json`'s layout of a finite float64: `f` format, or `e` below 1e-6 and from 1e21, with `e-07`
/// shortened to `e-7`. `None` for NaN and infinities, which `json.Marshal` refuses.
pub(crate) fn json_float(f: f64) -> Option<String> {
    if !f.is_finite() {
        return None;
    }
    let abs = f.abs();
    let fmt = if abs != 0.0 && (abs < 1e-6 || abs >= 1e21) {
        b'e'
    } else {
        b'f'
    };
    let mut s = format_float(f, fmt, 64);
    if fmt == b'e' {
        let b = s.as_bytes();
        let n = b.len();
        if n >= 4 && b[n - 4] == b'e' && b[n - 3] == b'-' && b[n - 2] == b'0' {
            let last = b[n - 1] as char;
            s.truncate(n - 2);
            s.push(last);
        }
    }
    Some(s)
}

/// Go's `strconv.Quote`: non-printable characters escaped, invalid UTF-8 bytes as `\x` escapes.
pub(crate) fn quote(s: &[u8]) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for chunk in s.utf8_chunks() {
        for r in chunk.valid().chars() {
            push_escaped(&mut out, r);
        }
        for b in chunk.invalid() {
            out.push_str(&format!("\\x{b:02x}"));
        }
    }
    out.push('"');
    out
}

fn push_escaped(out: &mut String, r: char) {
    if r == '"' || r == '\\' {
        out.push('\\');
        out.push(r);
        return;
    }
    if is_print(r) {
        out.push(r);
        return;
    }
    match r {
        '\x07' => out.push_str("\\a"),
        '\x08' => out.push_str("\\b"),
        '\x0c' => out.push_str("\\f"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        '\x0b' => out.push_str("\\v"),
        _ if (r as u32) < 0x20 || r as u32 == 0x7f => out.push_str(&format!("\\x{:02x}", r as u32)),
        _ if (r as u32) < 0x10000 => out.push_str(&format!("\\u{:04x}", r as u32)),
        _ => out.push_str(&format!("\\U{:08x}", r as u32)),
    }
}
