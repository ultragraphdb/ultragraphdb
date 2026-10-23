/*
 * Copyright 2015 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26, src/math/big/ftoa.go (Text, Append, roundShortest, fmtE, fmtF, fmtP) and decimal.go.
 * Changed for ultragraphdb: translated to Rust onto dashu-int; the 'g' and 'p' formats.
 * Copyright (c) 2026. Marvin Hansen.
 */

use dashu_base::BitTest;
use dashu_int::UBig;

use crate::types::big_float::{BigFloat, Form};

/// Go's `decimal`: digits without trailing zeros and the decimal point position (value = 0.mant × 10^exp).
struct Decimal {
    mant: Vec<u8>,
    exp: i64,
}

impl Decimal {
    /// Go's `decimal.init`: the exact digits of `m × 2^shift`.
    fn new(m: &UBig, shift: i64) -> Decimal {
        if *m == UBig::ZERO {
            return Decimal {
                mant: Vec::new(),
                exp: 0,
            };
        }
        let (n, exp10) = if shift >= 0 {
            (m << shift as usize, 0)
        } else {
            let tz = m.trailing_zeros().unwrap_or(0).min((-shift) as usize);
            let (m, shift) = (m >> tz, shift + tz as i64);
            (m * UBig::from(5u8).pow((-shift) as usize), shift)
        };
        let s = n.to_string();
        let digits = s.trim_end_matches('0');
        Decimal {
            mant: digits.as_bytes().to_vec(),
            exp: s.len() as i64 + exp10,
        }
    }

    fn at(&self, i: usize) -> u8 {
        self.mant.get(i).copied().unwrap_or(b'0')
    }

    fn should_round_up(&self, n: usize) -> bool {
        if self.mant[n] == b'5' && n + 1 == self.mant.len() {
            return n > 0 && (self.mant[n - 1] - b'0') & 1 != 0;
        }
        self.mant[n] >= b'5'
    }

    fn round(&mut self, n: i64) {
        if n < 0 || n as usize >= self.mant.len() {
            return;
        }
        if self.should_round_up(n as usize) {
            self.round_up(n as usize)
        } else {
            self.round_down(n as usize)
        }
    }

    fn round_up(&mut self, n: usize) {
        if n >= self.mant.len() {
            return;
        }
        let mut n = n;
        while n > 0 && self.mant[n - 1] >= b'9' {
            n -= 1;
        }
        if n == 0 {
            self.mant.truncate(1);
            self.mant[0] = b'1';
            self.exp += 1;
            return;
        }
        self.mant[n - 1] += 1;
        self.mant.truncate(n);
    }

    fn round_down(&mut self, n: usize) {
        if n >= self.mant.len() {
            return;
        }
        self.mant.truncate(n);
        while self.mant.last() == Some(&b'0') {
            self.mant.pop();
        }
        if self.mant.is_empty() {
            self.exp = 0;
        }
    }
}

impl BigFloat {
    /// Go's `MarshalText`: `Text('g', -1)`, the shortest decimal that rounds back to this value at its precision.
    pub fn marshal_text(&self) -> String {
        self.text_g(-1)
    }

    /// Go's `Text('g', prec)`; `String()` is `Text('g', 10)`.
    pub fn text_g(&self, prec: i64) -> String {
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
                return buf;
            }
            Form::Zero => (UBig::ZERO, 0),
            Form::Finite { mant, exp } => (mant.clone(), *exp),
        };
        let mut d = Decimal::new(&mant, exp);
        let shortest = prec < 0;
        let mut prec = prec;
        if shortest {
            self.round_shortest(&mut d, &mant, exp);
            prec = d.mant.len() as i64;
        } else {
            if prec == 0 {
                prec = 1;
            }
            d.round(prec);
        }
        let mut eprec = prec;
        if eprec > d.mant.len() as i64 && d.mant.len() as i64 >= d.exp {
            eprec = d.mant.len() as i64;
        }
        if shortest {
            eprec = 6;
        }
        let e = d.exp - 1;
        if e < -4 || e >= eprec {
            if prec > d.mant.len() as i64 {
                prec = d.mant.len() as i64;
            }
            fmt_e(&mut buf, prec - 1, &d);
        } else {
            if prec > d.exp {
                prec = d.mant.len() as i64;
            }
            fmt_f(&mut buf, (prec - d.exp).max(0), &d);
        }
        buf
    }

    /// Go's `roundShortest`: the shortest digits within the symmetric interval of half an ulp, digit by digit.
    fn round_shortest(&self, d: &mut Decimal, m: &UBig, exp: i64) {
        if d.mant.is_empty() {
            return;
        }
        let s = m.bit_len() as i64 - (i64::from(self.prec) + 1);
        let (mant, exp) = if s < 0 {
            (m << (-s) as usize, exp + s)
        } else {
            (m >> s as usize, exp + s)
        };
        let lower = Decimal::new(&(&mant - UBig::ONE), exp);
        let upper = Decimal::new(&(&mant + UBig::ONE), exp);
        let inclusive = !mant.bit(1);
        for i in 0..d.mant.len() {
            let m = d.mant[i];
            let l = lower.at(i);
            let u = upper.at(i);
            let okdown = l != m || inclusive && i + 1 == lower.mant.len();
            let okup = m != u && (inclusive || m + 1 < u || i + 1 < upper.mant.len());
            if okdown && okup {
                d.round(i as i64 + 1);
                return;
            } else if okdown {
                d.round_down(i + 1);
                return;
            } else if okup {
                d.round_up(i + 1);
                return;
            }
        }
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
fn fmt_e(buf: &mut String, prec: i64, d: &Decimal) {
    buf.push(if d.mant.is_empty() {
        '0'
    } else {
        d.mant[0] as char
    });
    if prec > 0 {
        buf.push('.');
        let mut i = 1usize;
        let m = d.mant.len().min(prec as usize + 1);
        if i < m {
            d.mant[i..m].iter().for_each(|&c| buf.push(c as char));
            i = m;
        }
        while i as i64 <= prec {
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
}

/// Go's `fmtF`: `ddddd.dddd`.
fn fmt_f(buf: &mut String, prec: i64, d: &Decimal) {
    if d.exp > 0 {
        let m = d.mant.len().min(d.exp as usize);
        d.mant[..m].iter().for_each(|&c| buf.push(c as char));
        (m..d.exp as usize).for_each(|_| buf.push('0'));
    } else {
        buf.push('0');
    }
    if prec > 0 {
        buf.push('.');
        for i in 0..prec {
            let j = d.exp + i;
            buf.push(if j >= 0 { d.at(j as usize) } else { b'0' } as char);
        }
    }
}
