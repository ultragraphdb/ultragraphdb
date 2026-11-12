/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/internal/strconv/atof.go, atoi.go, atob.go, ftoa.go and src/strconv/quote.go.
 * Changed for ultragraphdb: translated to Rust; the subset Dgraph calls. Shortest digits come from Rust's float
 * formatting, which finds the same shortest round-trip digits as Go's shortFloat (src/internal/strconv/uscale.go);
 * the port breaks a tie between two of them to the even one, as shortFloat does.
 * Copyright (c) 2026. Marvin Hansen.
 */

// Arithmetic on decoded text input is checked: an overflow here would be a malformed input
// accepted with a wrapped value, so it is refused instead (user decision 2026-11-12; the lint is not in the
// workspace table because elsewhere it flags provably bounded index math).
#![deny(clippy::arithmetic_side_effects)]

//! Go's `strconv`: `ParseUint`, `ParseInt` and `Atoi` (64-bit), `ParseBool` and `FormatBool`, `ParseFloat`,
//! `FormatFloat(f, 'e'|'E'|'f'|'g'|'G', -1, 64|32)`, `Quote`, `QuoteRune`, `Unquote` of a double-quoted string and
//! `IsPrint`.
//!
//! Go's strings hold any bytes, so the parsers and `quote` take anything that is bytes, a `str` included; a byte
//! that is not UTF-8 is quoted as `\x` and its hexadecimal value, as Go quotes it.

use std::ops::{Mul, Shl};

use dashu_int::UBig;
use ultragraphdb_error_num::{NumError, NumErrorEnum};

pub use crate::utils::isprint::is_print;

fn syntax_error(func: &'static str, s: &[u8]) -> NumError {
    NumError::syntax(func, quote(s))
}

fn range_error(func: &'static str, s: &[u8]) -> NumError {
    NumError::range(func, quote(s))
}

fn base_error(func: &'static str, s: &[u8], base: u32) -> NumError {
    NumError::base(func, quote(s), base)
}

fn lower(c: u8) -> u8 {
    c | (b'x' - b'X')
}

/// Go's `strconv.ParseUint(s, base, 64)` for base 0 (prefixes `0b`, `0o`, `0x`, a leading `0` for octal,
/// underscores between digits) or 2 to 36; any other base is refused with Go's `invalid base` error.
pub fn parse_uint<S: AsRef<[u8]> + ?Sized>(s: &S, base: u32) -> Result<u64, NumError> {
    parse_uint_as("ParseUint", s.as_ref(), base)
}

fn parse_uint_as(func: &'static str, s: &[u8], base: u32) -> Result<u64, NumError> {
    let Some((&first, tail)) = s.split_first() else {
        return Err(syntax_error(func, s));
    };
    let base0 = base == 0;
    // A base prefix counts only with a digit after it.
    let prefixed = tail
        .split_first()
        .filter(|(_, rest)| !rest.is_empty())
        .map(|(&c, rest)| (lower(c), rest));
    let (base, digits): (u64, &[u8]) = match (base0, first, prefixed) {
        (false, _, _) if (2..=36).contains(&base) => (u64::from(base), s),
        (false, _, _) => return Err(base_error(func, s, base)),
        (true, b'0', Some((b'b', rest))) => (2, rest),
        (true, b'0', Some((b'o', rest))) => (8, rest),
        (true, b'0', Some((b'x', rest))) => (16, rest),
        (true, b'0', _) => (8, tail),
        (true, _, _) => (10, s),
    };
    let mut underscores = false;
    let mut n: u64 = 0;
    for &c in digits {
        let d = match c {
            b'_' if base0 => {
                underscores = true;
                continue;
            }
            // A digit or a letter of either case, a letter from 10: Go's digit, `lower(c) - 'a' + 10`.
            _ => match char::from(c).to_digit(36) {
                Some(d) => u64::from(d),
                None => return Err(syntax_error(func, s)),
            },
        };
        if d >= base {
            return Err(syntax_error(func, s));
        }
        // Go's cutoff, `n >= maxUint64/base + 1`, is the product's overflow.
        n = n
            .checked_mul(base)
            .and_then(|n| n.checked_add(d))
            .ok_or_else(|| range_error(func, s))?;
    }
    if underscores && !underscore_ok(s) {
        return Err(syntax_error(func, s));
    }
    Ok(n)
}

fn parse_int_as(func: &'static str, s: &[u8], base: u32) -> Result<i64, NumError> {
    let (neg, rest) = match s.split_first() {
        None => return Err(syntax_error(func, s)),
        Some((b'+', rest)) => (false, rest),
        Some((b'-', rest)) => (true, rest),
        Some(_) => (false, s),
    };
    let un = match parse_uint_as(func, rest, base) {
        Ok(un) => un,
        Err(NumError(NumErrorEnum::Syntax { .. })) => return Err(syntax_error(func, s)),
        Err(NumError(NumErrorEnum::Range { .. })) => u64::MAX,
        Err(NumError(NumErrorEnum::Base { base, .. })) => return Err(base_error(func, s, base)),
    };
    let cutoff = 1u64 << 63;
    if !neg && un >= cutoff || neg && un > cutoff {
        return Err(range_error(func, s));
    }
    Ok(if neg {
        0i64.wrapping_sub_unsigned(un)
    } else {
        // Below 1 << 63 here, so the bits are the value.
        un.cast_signed()
    })
}

/// Go's `strconv.ParseInt(s, base, 64)` for base 0 (as [`parse_uint`]) or 2 to 36.
pub fn parse_int<S: AsRef<[u8]> + ?Sized>(s: &S, base: u32) -> Result<i64, NumError> {
    parse_int_as("ParseInt", s.as_ref(), base)
}

/// Go's `strconv.Atoi` on a 64-bit platform: `ParseInt(s, 10, 64)`, its errors naming `Atoi`.
pub fn atoi<S: AsRef<[u8]> + ?Sized>(s: &S) -> Result<i64, NumError> {
    parse_int_as("Atoi", s.as_ref(), 10)
}

/// Go's `strconv.ParseBool`.
pub fn parse_bool<S: AsRef<[u8]> + ?Sized>(s: &S) -> Result<bool, NumError> {
    match s.as_ref() {
        b"1" | b"t" | b"T" | b"TRUE" | b"true" | b"True" => Ok(true),
        b"0" | b"f" | b"F" | b"FALSE" | b"false" | b"False" => Ok(false),
        s => Err(syntax_error("ParseBool", s)),
    }
}

/// Go's `strconv.FormatBool`.
pub fn format_bool(b: bool) -> &'static str {
    if b { "true" } else { "false" }
}

/// Go's `strconv.ParseFloat(s, bit_size)`: decimal and hexadecimal literals with underscores between digits,
/// infinities and NaN. With `bit_size` 32 the value is the nearest float32, widened, as Go returns it.
pub fn parse_float<S: AsRef<[u8]> + ?Sized>(s: &S, bit_size: u32) -> Result<f64, NumError> {
    let s = s.as_ref();
    let bits = bit_size;
    let (val, n, range) = match special(s) {
        // Go's atof32 returns float32(math.NaN()), which widens to the quiet NaN without payload.
        Some((v, n)) if bits == 32 && v.is_nan() => {
            (f64::from_bits(0x7ff8_0000_0000_0000), n, false)
        }
        Some((v, n)) => (v, n, false),
        None => match read_float(s) {
            None => return Err(syntax_error("ParseFloat", s)),
            Some(r) => {
                let (v, range) = if r.hex {
                    atof_hex(r.mantissa, r.exp, r.neg, r.trunc, bits)
                        .ok_or_else(|| syntax_error("ParseFloat", s))?
                } else {
                    let read = s.get(..r.n).ok_or_else(|| syntax_error("ParseFloat", s))?;
                    atof_decimal(read, bits)
                };
                (v, r.n, range)
            }
        },
    };
    if n != s.len() {
        return Err(syntax_error("ParseFloat", s));
    }
    if range {
        return Err(range_error("ParseFloat", s));
    }
    Ok(val)
}

fn common_prefix_len_ignore_case(s: &[u8], prefix: &[u8]) -> usize {
    s.iter()
        .zip(prefix)
        .take_while(|(a, b)| a.to_ascii_lowercase() == **b)
        .count()
}

/// Go's `math.NaN()`, the NaN `special` returns: `Float64frombits(0x7FF8000000000001)`.
const GO_NAN: f64 = f64::from_bits(0x7ff8_0000_0000_0001);

/// Go's `special`: NaN, Inf and Infinity, signed, any case; the value and the bytes consumed.
fn special(s: &[u8]) -> Option<(f64, usize)> {
    let (infinity, nsign, rest) = match s.split_first()? {
        (b'+', rest) => (f64::INFINITY, 1_usize, rest),
        (b'-', rest) => (f64::NEG_INFINITY, 1, rest),
        (b'n' | b'N', _) => {
            return (common_prefix_len_ignore_case(s, b"nan") == 3).then_some((GO_NAN, 3));
        }
        (b'i' | b'I', _) => (f64::INFINITY, 0, s),
        _ => return None,
    };
    let mut n = common_prefix_len_ignore_case(rest, b"infinity");
    if 3 < n && n < 8 {
        n = 3;
    }
    if n != 3 && n != 8 {
        return None;
    }
    Some((infinity, nsign.checked_add(n)?))
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
    let (neg, i) = match s.first()? {
        b'+' => (false, 1),
        b'-' => (true, 1),
        _ => (false, 0),
    };
    // A `0x` counts only with a byte after it.
    let hex = matches!(s.get(i..)?, [b'0', x, _, ..] if x.eq_ignore_ascii_case(&b'x'));
    let (base, max_mant_digits, exp_char, i): (u64, i64, u8, usize) = if hex {
        (16, 16, b'p', i.checked_add(2)?)
    } else {
        (10, 19, b'e', i)
    };
    let Mantissa {
        mantissa,
        nd,
        mut nd_mant,
        mut dp,
        sawdot,
        sawdigits,
        trunc,
        mut underscores,
        mut i,
    } = read_mantissa(s, i, base, max_mant_digits)?;
    if !sawdigits {
        return None;
    }
    if !sawdot {
        dp = nd;
    }
    if base == 16 {
        dp = dp.checked_mul(4)?;
        nd_mant = nd_mant.checked_mul(4)?;
    }
    if s.get(i).is_some_and(|c| c.to_ascii_lowercase() == exp_char) {
        let (e, end) = read_exponent(s, i.checked_add(1)?, &mut underscores)?;
        dp = dp.checked_add(e)?;
        i = end;
    } else if base == 16 {
        return None;
    }
    let exp = if mantissa != 0 {
        dp.checked_sub(nd_mant)?
    } else {
        0
    };
    if underscores && !underscore_ok(s.get(..i)?) {
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

/// The mantissa `readFloat` reads after the sign and base prefix: its leading digits, its digit counts, its
/// decimal point, and where it ends.
struct Mantissa {
    mantissa: u64,
    nd: i64,
    nd_mant: i64,
    dp: i64,
    sawdot: bool,
    sawdigits: bool,
    trunc: bool,
    underscores: bool,
    i: usize,
}

/// `readFloat`'s digit loop, from `i`: digits of `base`, one dot and underscores, up to the first other byte; `None`
/// where a count overflows.
fn read_mantissa(s: &[u8], i: usize, base: u64, max_mant_digits: i64) -> Option<Mantissa> {
    let mut m = Mantissa {
        mantissa: 0,
        nd: 0,
        nd_mant: 0,
        dp: 0,
        sawdot: false,
        sawdigits: false,
        trunc: false,
        underscores: false,
        i,
    };
    for &c in s.get(i..)? {
        let lc = c.to_ascii_lowercase();
        if c == b'_' {
            m.underscores = true;
        } else if c == b'.' {
            if m.sawdot {
                break;
            }
            m.sawdot = true;
            m.dp = m.nd;
        } else if c.is_ascii_digit() || base == 16 && (b'a'..=b'f').contains(&lc) {
            m.sawdigits = true;
            if c == b'0' && m.nd == 0 {
                // A leading zero moves the point and counts as no digit.
                m.dp = m.dp.checked_sub(1)?;
            } else {
                m.nd = m.nd.checked_add(1)?;
                if m.nd_mant < max_mant_digits {
                    // At most 19 decimal or 16 hexadecimal digits, so the mantissa cannot wrap.
                    let d = u64::from(char::from(lc).to_digit(16)?);
                    m.mantissa = m.mantissa.wrapping_mul(base).wrapping_add(d);
                    m.nd_mant = m.nd_mant.checked_add(1)?;
                } else if c != b'0' {
                    m.trunc = true;
                }
            }
        } else {
            break;
        }
        m.i = m.i.checked_add(1)?;
    }
    Some(m)
}

/// `readFloat`'s exponent, from `i` just past the exponent character: its signed value, digits past 10000 ignored,
/// and where it ends; `None` without a digit. An underscore sets `underscores`.
fn read_exponent(s: &[u8], i: usize, underscores: &mut bool) -> Option<(i64, usize)> {
    let rest = s.get(i..)?;
    let (negative, digits) = match rest.split_first()? {
        (b'+', digits) => (false, digits),
        (b'-', digits) => (true, digits),
        _ => (false, rest),
    };
    if !digits.first()?.is_ascii_digit() {
        return None;
    }
    let used = digits
        .iter()
        .take_while(|&&c| c.is_ascii_digit() || c == b'_')
        .count();
    let mut e: i64 = 0;
    for &c in digits.get(..used)? {
        if c == b'_' {
            *underscores = true;
        } else if e < 10000 {
            let d = i64::from(char::from(c).to_digit(10)?);
            e = e.checked_mul(10)?.checked_add(d)?;
        }
    }
    // `digits` ends `s`, so its start is the length before it.
    let end = s.len().checked_sub(digits.len())?.checked_add(used)?;
    Some((if negative { e.checked_neg()? } else { e }, end))
}

/// Go's `underscoreOK`: underscores only between digits, or between a base prefix and a digit.
fn underscore_ok(s: &[u8]) -> bool {
    let s = match s.split_first() {
        Some((b'-' | b'+', rest)) => rest,
        _ => s,
    };
    let (mut saw, hex, rest) = match s {
        [b'0', prefix, rest @ ..] if matches!(prefix.to_ascii_lowercase(), b'b' | b'o' | b'x') => {
            (b'0', prefix.eq_ignore_ascii_case(&b'x'), rest)
        }
        _ => (b'^', false, s),
    };
    for &c in rest {
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
    }
    saw != b'_'
}

/// A decimal float: Rust's parse is correctly rounded, as Go's unrounded scaling with its exact fallback is.
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

/// Go's `atofHex`: rounds a hexadecimal mantissa and binary exponent to the float's bits, half to even; `None` where
/// the exponent overflows.
fn atof_hex(mut mantissa: u64, exp: i64, neg: bool, trunc: bool, bits: u32) -> Option<(f64, bool)> {
    // Go's float32info and float64info, with the exponents they bound: `bias + 1` and `1 << expbits + bias - 2`.
    let (mantbits, expbits, bias, min_exp, max_exp): (u32, u32, i64, i64, i64) = if bits == 32 {
        (23, 8, -127, -126, 127)
    } else {
        (52, 11, -1023, -1022, 1023)
    };
    // The mantissa is rounded with two bits below it, and normalised while a third sits above it.
    let (rounding, above) = (mantbits.checked_add(2)?, mantbits.checked_add(3)?);
    let mut exp = exp.checked_add(i64::from(mantbits))?;
    while mantissa != 0 && mantissa >> rounding == 0 {
        mantissa <<= 1;
        exp = exp.checked_sub(1)?;
    }
    if trunc {
        mantissa |= 1;
    }
    while mantissa >> above != 0 {
        mantissa = (mantissa >> 1) | (mantissa & 1);
        exp = exp.checked_add(1)?;
    }
    let denormal = min_exp.checked_sub(2)?;
    while mantissa > 1 && exp < denormal {
        mantissa = (mantissa >> 1) | (mantissa & 1);
        exp = exp.checked_add(1)?;
    }
    let mut round = mantissa & 3;
    mantissa >>= 2;
    round |= mantissa & 1;
    exp = exp.checked_add(2)?;
    if round == 3 {
        mantissa = mantissa.checked_add(1)?;
        if mantissa == 1 << mantbits.checked_add(1)? {
            mantissa >>= 1;
            exp = exp.checked_add(1)?;
        }
    }
    if mantissa >> mantbits == 0 {
        exp = bias;
    }
    let mut range = false;
    if exp > max_exp {
        mantissa = 1 << mantbits;
        exp = max_exp.checked_add(1)?;
        range = true;
    }
    let mut b = mantissa & !(u64::MAX << mantbits);
    b |= (exp.checked_sub(bias)?.cast_unsigned() & !(u64::MAX << expbits)) << mantbits;
    if neg {
        b |= 1u64 << mantbits << expbits;
    }
    if bits == 32 {
        // A float32's bits are the low four bytes.
        let [b0, b1, b2, b3, ..] = b.to_le_bytes();
        let f = f32::from_bits(u32::from_le_bytes([b0, b1, b2, b3]));
        Some((f64::from(f), range))
    } else {
        Some((f64::from_bits(b), range))
    }
}

/// The shortest decimal digits of a finite, nonzero float and the position of the decimal point, Go's
/// `decimalSlice` (`d` and `dp`: the value is 0.d × 10^dp).
///
/// Rust's shortest search and Go's agree except where two shortest candidates lie exactly as far from the value;
/// Go takes the even one (its digits are the exact value rounded half to even), so that case is redone here.
fn shortest_digits(f: f64, bits: u32) -> Option<(Vec<u8>, i64)> {
    let sci = if bits == 32 {
        format!("{:e}", (f as f32).abs())
    } else {
        format!("{:e}", f.abs())
    };
    // `{:e}` writes `d.ddde-x`: the digits before the `e`, the exponent after it as a decimal integer.
    let (mantissa, exponent) = sci.split_once('e')?;
    let digits: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).collect();
    let dp = exponent.parse::<i64>().ok()?.checked_add(1)?;
    let v = if bits == 32 {
        f64::from(f as f32).abs()
    } else {
        f.abs()
    };
    if let Some((exact, edp)) = exact_digits(v)
        && exact.len() > digits.len()
        && let Some((r, rdp)) = round_half_even(&exact, edp, digits.len())
        && (r.as_slice(), rdp) != (digits.as_slice(), dp)
        && round_trips(&r, rdp, v, bits)
    {
        return Some((r, rdp));
    }
    Some((digits, dp))
}

/// The exact decimal digits of a positive finite float and their decimal point; `None` where the shift or power,
/// at most 1,074, does not fit a `usize`.
fn exact_digits(v: f64) -> Option<(Vec<u8>, i64)> {
    let b = v.to_bits();
    // Eleven bits, so the value is the bits.
    let biased = ((b >> 52) & 0x7ff).cast_signed();
    let frac = b & ((1 << 52) - 1);
    let (m, e) = if biased == 0 {
        (frac, -1074)
    } else {
        (frac | (1 << 52), biased.checked_sub(1075)?)
    };
    let m = UBig::from(m);
    let shift = usize::try_from(e.unsigned_abs()).ok()?;
    // A `UBig` grows rather than overflowing, so its operators are called by name: the lint cannot tell it from a
    // fixed-width integer.
    let (n, exp10) = if e >= 0 {
        (Shl::shl(m, shift), 0)
    } else {
        (Mul::mul(m, UBig::from(5u8).pow(shift)), e)
    };
    let s = n.to_string();
    let t = s.trim_end_matches('0');
    Some((
        t.as_bytes().to_vec(),
        i64::try_from(s.len()).ok()?.checked_add(exp10)?,
    ))
}

/// Digits rounded half to even to `n` digits, trailing zeros trimmed; `None` unless `d` holds more than `n`.
fn round_half_even(d: &[u8], dp: i64, n: usize) -> Option<(Vec<u8>, i64)> {
    let (kept, rest) = d.split_at_checked(n)?;
    let (&next, after) = rest.split_first()?;
    let rest_zero = after.iter().all(|&c| c == b'0');
    // An ASCII digit has its value's parity.
    let odd = kept.last().is_some_and(|&c| c & 1 == 1);
    let up = next > b'5' || next == b'5' && (!rest_zero || odd);
    let mut r = kept.to_vec();
    let mut dp = dp;
    if up {
        // The carry turns the trailing nines to zeros and raises the digit before them; nines throughout become a
        // 1 and zeros, one digit fewer, the point one further.
        match r.iter().rposition(|&c| c != b'9') {
            Some(at) => {
                if let Some((raised, nines)) = r.get_mut(at..).and_then(<[u8]>::split_first_mut) {
                    *raised = raised.checked_add(1)?;
                    nines.fill(b'0');
                }
            }
            None => {
                r.fill(b'0');
                r.insert(0, b'1');
                let _dropped_zero: Option<u8> = r.pop();
                dp = dp.checked_add(1)?;
            }
        }
    }
    while r.last() == Some(&b'0') {
        let _trailing_zero: Option<u8> = r.pop();
    }
    Some((r, dp))
}

fn round_trips(d: &[u8], dp: i64, v: f64, bits: u32) -> bool {
    let digits: String = d.iter().map(|&c| char::from(c)).collect();
    let text = format!("0.{digits}e{dp}");
    if bits == 32 {
        text.parse::<f32>().is_ok_and(|x| f64::from(x) == v)
    } else {
        text.parse::<f64>().is_ok_and(|x| x == v)
    }
}

/// Go's `strconv.FormatFloat(f, fmt, -1, bit_size)` for `fmt` one of `e`, `E`, `f`, `g`, `G`: the shortest digits
/// that read back as `f`, as a float32 when `bit_size` is 32. Any other `fmt` gives `%` and the byte, Go's text for
/// an unknown format; Go's `b`, `x` and `X` are not ported.
pub fn format_float(f: f64, fmt: u8, bit_size: u32) -> String {
    let bits = bit_size;
    let f = if bits == 32 { f64::from(f as f32) } else { f };
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
    // `None` is unreachable: the decimal exponent is within ±324 and the digits at most 767. Rust's shortest text
    // of the value stands in for Go's should it be reached.
    format_finite(f, fmt, bits).unwrap_or_else(|| f.to_string())
}

/// [`format_float`] of a finite `f`; `None` where a count overflows.
fn format_finite(f: f64, fmt: u8, bits: u32) -> Option<String> {
    let neg = f.is_sign_negative();
    let (d, dp) = if f == 0.0 {
        (Vec::new(), 0)
    } else {
        shortest_digits(f, bits)?
    };
    let nd = i64::try_from(d.len()).ok()?;
    let mut out = String::new();
    match fmt {
        b'e' | b'E' => fmt_e(&mut out, neg, &d, dp, nd.checked_sub(1)?, fmt)?,
        b'f' => fmt_f(&mut out, neg, &d, dp, nd.checked_sub(dp)?.max(0))?,
        b'g' | b'G' => {
            let prec = nd;
            let exp = dp.checked_sub(1)?;
            if !(-4..6).contains(&exp) {
                let e = if fmt == b'G' { b'E' } else { b'e' };
                fmt_e(&mut out, neg, &d, dp, prec.checked_sub(1)?, e)?;
            } else {
                fmt_f(&mut out, neg, &d, dp, prec.checked_sub(dp)?.max(0))?;
            }
        }
        // Unknown format.
        _ => {
            out.push('%');
            out.push(char::from(fmt));
        }
    }
    Some(out)
}

fn fmt_e(out: &mut String, neg: bool, d: &[u8], dp: i64, prec: i64, fmt: u8) -> Option<()> {
    if neg {
        out.push('-');
    }
    let (first, rest) = d
        .split_first()
        .map_or(('0', &[][..]), |(&c, rest)| (char::from(c), rest));
    out.push(first);
    if let Some(prec) = usize::try_from(prec).ok().filter(|&prec| prec > 0) {
        out.push('.');
        // `prec` digits past the point: those after the first, then zeros.
        let shown = rest.get(..prec).unwrap_or(rest);
        shown.iter().for_each(|&c| out.push(char::from(c)));
        (shown.len()..prec).for_each(|_| out.push('0'));
    }
    out.push(char::from(fmt));
    let exp = if d.is_empty() { 0 } else { dp.checked_sub(1)? };
    out.push(if exp < 0 { '-' } else { '+' });
    let exp = exp.unsigned_abs();
    if exp < 10 {
        out.push('0');
    }
    out.push_str(&exp.to_string());
    Some(())
}

fn fmt_f(out: &mut String, neg: bool, d: &[u8], dp: i64, prec: i64) -> Option<()> {
    if neg {
        out.push('-');
    }
    if let Some(whole) = usize::try_from(dp).ok().filter(|&whole| whole > 0) {
        let shown = d.get(..whole).unwrap_or(d);
        shown.iter().for_each(|&c| out.push(char::from(c)));
        (shown.len()..whole).for_each(|_| out.push('0'));
    } else {
        out.push('0');
    }
    if prec > 0 {
        out.push('.');
        for i in 0..prec {
            let j = dp.checked_add(i)?;
            out.push(
                usize::try_from(j)
                    .ok()
                    .and_then(|j| d.get(j))
                    .map_or('0', |&c| char::from(c)),
            );
        }
    }
    Some(())
}

/// Go's `strconv.Quote`: the bytes in double quotes, the runes Go's `IsPrint` rejects escaped and each byte that is
/// not UTF-8 as a `\x` escape.
pub fn quote<S: AsRef<[u8]> + ?Sized>(s: &S) -> String {
    let s = s.as_ref();
    // A capacity hint: the bytes and their quotes.
    let mut out = String::with_capacity(s.len().saturating_add(2));
    out.push('"');
    for chunk in s.utf8_chunks() {
        for r in chunk.valid().chars() {
            push_escaped(&mut out, r, '"');
        }
        for b in chunk.invalid() {
            out.push_str(&format!("\\x{b:02x}"));
        }
    }
    out.push('"');
    out
}

/// Go's `strconv.QuoteRune`: the rune in single quotes, escaped as [`quote`] escapes it.
pub fn quote_rune(r: char) -> String {
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
    // The backslash and an ASCII byte: a character boundary.
    let rest = s.get(2..)?;
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
            // Hexadecimal digits are ASCII: a character boundary.
            let tail = rest.get(n..)?;
            if e == b'x' {
                // A single byte, possibly not UTF-8.
                return Some((vec![u8::try_from(v).ok()?], tail));
            }
            Some((char::from_u32(v)?.to_string().into_bytes(), tail))
        }
        b'0'..=b'7' => {
            let digits = rest.as_bytes().get(..2)?;
            let v = digits
                .iter()
                .try_fold(char::from(e).to_digit(8)?, |v, &d| {
                    Some(v << 3 | char::from(d).to_digit(8)?)
                })?;
            // Octal digits are ASCII: a character boundary.
            let tail = rest.get(2..)?;
            u8::try_from(v).ok().map(|v| (vec![v], tail))
        }
        _ => None,
    }
}

/// Go's `strconv.Unquote` of a double-quoted string, as bytes: `\x` and octal escapes may write bytes that are
/// not UTF-8. `None` is Go's `ErrSyntax`.
pub fn unquote(s: &str) -> Option<Vec<u8>> {
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
