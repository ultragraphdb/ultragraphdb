/*
 * Copyright 2015 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/math/big/ftoa.go (Text, Append, roundShortest, fmtE, fmtF, fmtP) and decimal.go.
 * Changed for ultragraphdb: translated to Rust onto dashu-int; the 'g' and 'p' formats.
 * Copyright (c) 2026. Marvin Hansen.
 */

use dashu_int::UBig;
use dashu_int::ops::BitTest;

use crate::errors::types_error::TypesError;
use crate::types::big_float::{BigFloat, Form, go_int, go_uint};

/// Go's `decimal`: digits without trailing zeros and the decimal point position (value = 0.mant × 10^exp).
struct Decimal {
    mant: Vec<u8>,
    exp: i64,
}

impl Decimal {
    /// Go's `decimal.init`: the exact digits of `m × 2^shift`.
    fn new(m: &UBig, shift: i64) -> Result<Decimal, TypesError> {
        if *m == UBig::ZERO {
            return Ok(Decimal {
                mant: Vec::new(),
                exp: 0,
            });
        }
        let down = go_uint(shift.unsigned_abs())?;
        let (n, exp10) = if shift >= 0 {
            (m << down, 0)
        } else {
            let tz = m.trailing_zeros().unwrap_or(0).min(down);
            (
                (m >> tz) * UBig::from(5u8).pow(down - tz),
                shift + go_int(tz),
            )
        };
        let s = n.to_string();
        let digits = s.trim_end_matches('0');
        Ok(Decimal {
            mant: digits.as_bytes().to_vec(),
            exp: go_int(s.len()) + exp10,
        })
    }

    fn at(&self, i: usize) -> u8 {
        self.mant.get(i).copied().unwrap_or(b'0')
    }

    fn should_round_up(&self, n: usize) -> bool {
        match self.mant.get(n) {
            Some(b'5') if n + 1 == self.mant.len() => n
                .checked_sub(1)
                .and_then(|p| self.mant.get(p))
                .is_some_and(|&p| (p - b'0') & 1 != 0),
            Some(&c) => c >= b'5',
            None => false,
        }
    }

    fn round(&mut self, n: i64) {
        let Ok(n) = usize::try_from(n) else {
            return;
        };
        if n >= self.mant.len() {
            return;
        }
        if self.should_round_up(n) {
            self.round_up(n)
        } else {
            self.round_down(n)
        }
    }

    fn round_up(&mut self, n: usize) {
        if n >= self.mant.len() {
            return;
        }
        let mut n = n;
        while n > 0 && self.mant.get(n - 1).is_some_and(|&c| c >= b'9') {
            n -= 1;
        }
        if n == 0 {
            self.mant.truncate(1);
            if let Some(first) = self.mant.first_mut() {
                *first = b'1';
            }
            self.exp += 1;
            return;
        }
        if let Some(c) = self.mant.get_mut(n - 1) {
            *c += 1;
        }
        self.mant.truncate(n);
    }

    fn round_down(&mut self, n: usize) {
        if n >= self.mant.len() {
            return;
        }
        self.mant.truncate(n);
        while self.mant.last() == Some(&b'0') {
            let _zero: Option<u8> = self.mant.pop();
        }
        if self.mant.is_empty() {
            self.exp = 0;
        }
    }
}

impl BigFloat {
    /// Go's `MarshalText`: `Text('g', -1)`, the shortest decimal that rounds back to this value at its precision.
    pub fn marshal_text(&self) -> Result<String, TypesError> {
        self.text_g(-1)
    }

    /// Go's `Text('g', prec)`; `String()` is `Text('g', 10)`.
    pub fn text_g(&self, prec: i64) -> Result<String, TypesError> {
        let mut buf = String::new();
        if self.neg {
            buf.push('-');
        }
        let (mant, exp) = match &self.form {
            Form::Inf => {
                if !self.neg {
                    buf.push('+');
                }
                buf.push_str("Inf");
                return Ok(buf);
            }
            Form::Zero => (UBig::ZERO, 0),
            Form::Finite { mant, exp } => (mant.clone(), *exp),
        };
        let mut d = Decimal::new(&mant, exp)?;
        let shortest = prec < 0;
        let mut prec = prec;
        if shortest {
            self.round_shortest(&mut d, &mant, exp)?;
            prec = go_int(d.mant.len());
        } else {
            if prec == 0 {
                prec = 1;
            }
            d.round(prec);
        }
        let mut eprec = prec;
        if eprec > go_int(d.mant.len()) && go_int(d.mant.len()) >= d.exp {
            eprec = go_int(d.mant.len());
        }
        if shortest {
            eprec = 6;
        }
        let e = d.exp - 1;
        if e < -4 || e >= eprec {
            if prec > go_int(d.mant.len()) {
                prec = go_int(d.mant.len());
            }
            fmt_e(&mut buf, prec - 1, &d)?;
        } else {
            if prec > d.exp {
                prec = go_int(d.mant.len());
            }
            fmt_f(&mut buf, (prec - d.exp).max(0), &d)?;
        }
        Ok(buf)
    }

    /// Go's `roundShortest`: the shortest digits within the symmetric interval of half an ulp, digit by digit.
    fn round_shortest(&self, d: &mut Decimal, m: &UBig, exp: i64) -> Result<(), TypesError> {
        if d.mant.is_empty() {
            return Ok(());
        }
        let s = go_int(m.bit_len()) - (i64::from(self.prec) + 1);
        let bits = go_uint(s.unsigned_abs())?;
        let (mant, exp) = if s < 0 {
            (m << bits, exp + s)
        } else {
            (m >> bits, exp + s)
        };
        let lower = Decimal::new(&(&mant - UBig::ONE), exp)?;
        let upper = Decimal::new(&(&mant + UBig::ONE), exp)?;
        let inclusive = !mant.bit(1);
        for i in 0..d.mant.len() {
            let Some(&digit) = d.mant.get(i) else {
                break;
            };
            let l = lower.at(i);
            let u = upper.at(i);
            let okdown = l != digit || inclusive && i + 1 == lower.mant.len();
            // Past upper's trimmed mantissa `at` reads '0', but earlier digits fixed the bound, so rounding up is
            // valid unless the digit is '9' (go.dev/issue/80206).
            let okup = digit != u
                && (inclusive
                    || digit + 1 < u
                    || i + 1 < upper.mant.len()
                    || i >= upper.mant.len() && digit < b'9');
            if okdown && okup {
                d.round(go_int(i) + 1);
                return Ok(());
            } else if okdown {
                d.round_down(i + 1);
                return Ok(());
            } else if okup {
                d.round_up(i + 1);
                return Ok(());
            }
        }
        Ok(())
    }

    /// Go's `Text('p', 0)`: `0x.<hex mantissa>p<exponent>`, the exact value.
    pub fn text_p(&self) -> String {
        let mut buf = String::new();
        if self.neg {
            buf.push('-');
        }
        match &self.form {
            Form::Inf => {
                if !self.neg {
                    buf.push('+');
                }
                buf.push_str("Inf");
            }
            Form::Zero => buf.push('0'),
            Form::Finite { mant, .. } => {
                let n = mant.bit_len();
                let aligned = mant << ((4 - n % 4) % 4);
                let hex = format!("{:x}", aligned);
                buf.push_str("0x.");
                buf.push_str(hex.trim_end_matches('0'));
                buf.push('p');
                let e = self.go_exp();
                if e >= 0 {
                    buf.push('+');
                }
                buf.push_str(&e.to_string());
            }
        }
        buf
    }
}

/// Go's `fmtE`: `d.dddde±dd`.
fn fmt_e(buf: &mut String, prec: i64, d: &Decimal) -> Result<(), TypesError> {
    buf.push(d.mant.first().map_or('0', |&c| c as char));
    if prec > 0 {
        buf.push('.');
        // The digits after the first, at most `prec` of them.
        let shown = d.mant.iter().skip(1).take(go_uint(prec.unsigned_abs())?);
        let mut i = 1usize;
        for &c in shown {
            buf.push(c as char);
            i += 1;
        }
        // Pads with zeros to `prec` digits: `i` rises by one a turn, and the loop ends past `prec`.
        while go_int(i) <= prec {
            buf.push('0');
            i += 1;
        }
    }
    buf.push('e');
    let mut exp = if d.mant.is_empty() { 0 } else { d.exp - 1 };
    if exp < 0 {
        buf.push('-');
        exp = -exp;
    } else {
        buf.push('+');
    }
    if exp < 10 {
        buf.push('0');
    }
    buf.push_str(&exp.to_string());
    Ok(())
}

/// Go's `fmtF`: `ddddd.dddd`.
fn fmt_f(buf: &mut String, prec: i64, d: &Decimal) -> Result<(), TypesError> {
    if d.exp > 0 {
        let exp = go_uint(d.exp.unsigned_abs())?;
        d.mant.iter().take(exp).for_each(|&c| buf.push(c as char));
        (d.mant.len()..exp).for_each(|_| buf.push('0'));
    } else {
        buf.push('0');
    }
    if prec > 0 {
        buf.push('.');
        for i in 0..prec {
            let digit = usize::try_from(d.exp + i).map_or(b'0', |j| d.at(j));
            buf.push(digit as char);
        }
    }
    Ok(())
}
