/*
 * Copyright 2015 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26, src/math/big/floatconv.go (Parse, scan, pow5), natconv.go (nat.scan), ratconv.go
 * (scanExponent) and intconv.go (scanSign). Changed for ultragraphdb: translated to Rust onto dashu-int.
 * Copyright (c) 2026. Marvin Hansen.
 */

use dashu_base::BitTest;
use dashu_int::UBig;

use crate::types::big_float::{BigFloat, Form, MAX_EXP, MIN_EXP};
use ultragraphdb_common_utils::go_strconv;

const ERR_NO_DIGITS: &str = "number has no digits";
const ERR_INVAL_SEP: &str = "'_' must separate successive digits";

/// Go's `strings.Reader` as an `io.ByteScanner`.
struct Scanner<'a> {
    s: &'a [u8],
    pos: usize,
}

impl Scanner<'_> {
    fn read(&mut self) -> Option<u8> {
        let c = self.s.get(self.pos).copied();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn unread(&mut self) {
        self.pos -= 1;
    }
}

/// Go's `nat.scan(r, 0, true)`: the mantissa digits, the base, the fraction digit count (`<= 0`) or digit count.
fn scan_mantissa(r: &mut Scanner<'_>) -> Result<(UBig, u32, i64), &'static str> {
    let mut prev = b'.';
    let mut inval_sep = false;
    let mut ch = r.read();
    let mut b: u32 = 10;
    let mut prefix = 0u8;
    let mut count: i64 = 0;
    if ch == Some(b'0') {
        prev = b'0';
        count = 1;
        ch = r.read();
        if let Some(c) = ch {
            match c {
                b'b' | b'B' => (b, prefix) = (2, b'b'),
                b'o' | b'O' => (b, prefix) = (8, b'o'),
                b'x' | b'X' => (b, prefix) = (16, b'x'),
                _ => {}
            }
            if prefix != 0 {
                count = 0;
                ch = r.read();
            }
        }
    }
    let mut frac_ok = true;
    let mut digits: Vec<u8> = Vec::new();
    let mut dp: i64 = -1;
    while let Some(c) = ch {
        if c == b'.' && frac_ok {
            frac_ok = false;
            if prev == b'_' {
                inval_sep = true;
            }
            prev = b'.';
            dp = count;
        } else if c == b'_' {
            if prev != b'0' {
                inval_sep = true;
            }
            prev = b'_';
        } else {
            let d1 = match c {
                b'0'..=b'9' => u32::from(c - b'0'),
                b'a'..=b'z' => u32::from(c - b'a') + 10,
                b'A'..=b'Z' => u32::from(c - b'A') + 10,
                _ => u32::MAX,
            };
            if d1 >= b {
                r.unread();
                break;
            }
            prev = b'0';
            count += 1;
            digits.push(c);
        }
        ch = r.read();
    }
    // Go sets the separator error first and lets "no digits" replace it.
    if count == 0 {
        return Err(ERR_NO_DIGITS);
    }
    if inval_sep || prev == b'_' {
        return Err(ERR_INVAL_SEP);
    }
    let text = std::str::from_utf8(&digits).expect("ASCII digits");
    let mant = if text.is_empty() {
        UBig::ZERO
    } else {
        UBig::from_str_radix(text, b).expect("digits of base b")
    };
    if dp >= 0 {
        count = dp - count;
    }
    Ok((mant, b, count))
}

/// Go's `scanExponent(r, true, true)`: the exponent and its base, 10 for `e`/`E`, 2 for `p`/`P`.
fn scan_exponent(r: &mut Scanner<'_>) -> Result<(i64, u32), String> {
    let Some(ch) = r.read() else {
        return Ok((0, 10));
    };
    let base = match ch {
        b'e' | b'E' => 10,
        b'p' | b'P' => 2,
        _ => {
            r.unread();
            return Ok((0, 10));
        }
    };
    let mut digits = String::new();
    let mut ch = r.read();
    if let Some(c @ (b'+' | b'-')) = ch {
        if c == b'-' {
            digits.push('-');
        }
        ch = r.read();
    }
    let mut prev = b'.';
    let mut inval_sep = false;
    let mut has_digits = false;
    while let Some(c) = ch {
        if c.is_ascii_digit() {
            digits.push(c as char);
            prev = b'0';
            has_digits = true;
        } else if c == b'_' {
            if prev != b'0' {
                inval_sep = true;
            }
            prev = b'_';
        } else {
            r.unread();
            break;
        }
        ch = r.read();
    }
    if !has_digits {
        return Err(ERR_NO_DIGITS.into());
    }
    let exp = go_strconv::parse_int(&digits, 10).map_err(|e| e.to_string())?;
    if inval_sep || prev == b'_' {
        return Err(ERR_INVAL_SEP.into());
    }
    Ok((exp, base))
}

impl BigFloat {
    /// Go's `pow5`: 5^n at this precision, multiplying by squares of 5 held at 64 more bits.
    fn pow5(n: u64, prec: u32) -> BigFloat {
        const POW5_MAX: u64 = 27;
        if n <= POW5_MAX {
            return BigFloat::from_parts(false, UBig::from(5u8).pow(n as usize), 0, prec);
        }
        let mut z = BigFloat::from_parts(false, UBig::from(5u8).pow(POW5_MAX as usize), 0, prec);
        let mut f = BigFloat::from_parts(false, UBig::from(5u8), 0, prec + 64);
        let mut n = n - POW5_MAX;
        while n > 0 {
            if n & 1 != 0 {
                z = z.mul(&f, prec);
            }
            f = f.mul(&f, prec + 64);
            n >>= 1;
        }
        z
    }

    /// Go's `Float.scan` with base 0: the sign, mantissa, exponent, and the decimal exponent applied through
    /// `pow5`, which is not always correctly rounded.
    fn scan(r: &mut Scanner<'_>, prec: u32) -> Result<BigFloat, String> {
        let neg = match r.read() {
            None => return Err("EOF".into()),
            Some(b'-') => true,
            Some(b'+') => false,
            Some(_) => {
                r.unread();
                false
            }
        };
        let (mant, b, fcount) = scan_mantissa(r).map_err(String::from)?;
        let (exp, ebase) = scan_exponent(r)?;
        if mant == UBig::ZERO {
            return Ok(BigFloat {
                prec,
                neg,
                form: Form::Zero,
            });
        }
        let mut exp2: i64 = 0;
        let mut exp5: i64 = 0;
        if fcount < 0 {
            match b {
                10 => {
                    exp5 = fcount;
                    exp2 += fcount;
                }
                2 => exp2 += fcount,
                8 => exp2 += fcount * 3,
                _ => exp2 += fcount * 4,
            }
        }
        if ebase == 10 {
            exp5 += exp;
        }
        exp2 += exp;
        let go_exp = mant.bit_len() as i64 + exp2;
        if !(MIN_EXP..=MAX_EXP).contains(&go_exp) {
            return Err("exponent overflow".into());
        }
        let z = BigFloat {
            prec,
            neg,
            form: Form::Finite { mant, exp: exp2 },
        };
        if exp5 == 0 {
            let Form::Finite { mant, exp } = z.form else {
                unreachable!("finite")
            };
            return Ok(BigFloat::from_parts(neg, mant, exp, prec));
        }
        let p = BigFloat::pow5(exp5.unsigned_abs(), prec + 64);
        Ok(if exp5 < 0 {
            z.quo(&p, prec)
        } else {
            z.mul(&p, prec)
        })
    }

    /// Go's `Float.Parse(s, 0)`: `Inf` and `inf` with an optional sign, or a number by `scan` that must consume
    /// all of `s`. The error is the inner one, without Go's `UnmarshalText` wrapping.
    pub(crate) fn parse(s: &[u8], prec: u32) -> Result<BigFloat, String> {
        match s {
            b"Inf" | b"inf" | b"+Inf" | b"+inf" => return Ok(BigFloat::inf(false, prec)),
            b"-Inf" | b"-inf" => return Ok(BigFloat::inf(true, prec)),
            _ => {}
        }
        let mut r = Scanner { s, pos: 0 };
        let f = BigFloat::scan(&mut r, prec)?;
        if let Some(c) = r.read() {
            return Err(format!(
                "expected end of string, found {}",
                go_strconv::quote_rune(char::from(c))
            ));
        }
        Ok(f)
    }

    /// Go's `UnmarshalText` at a precision, with its error text.
    pub fn unmarshal_text(text: impl AsRef<[u8]>, prec: u32) -> Result<BigFloat, String> {
        let text = text.as_ref();
        BigFloat::parse(text, prec).map_err(|e| {
            format!(
                "math/big: cannot unmarshal {} into a *big.Float ({e})",
                go_strconv::quote(text)
            )
        })
    }
}
