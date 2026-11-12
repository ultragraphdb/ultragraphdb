/*
 * Copyright 2014 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/math/big/float.go. Changed for ultragraphdb: translated to Rust onto dashu-int's
 * integers; the part of big.Float Dgraph's bigfloat values use, with Go's default rounding (to nearest even).
 * Copyright (c) 2026. Marvin Hansen.
 */

mod big_float_arith;
mod big_float_format;
mod big_float_scan;

use dashu_int::UBig;
use dashu_int::ops::BitTest;

use crate::errors::types_error::TypesError;

/// Go's `big.MinExp` and `big.MaxExp`: the range of the exponent `e` of a value `0.m × 2^e`.
const MIN_EXP: i64 = i32::MIN as i64;
const MAX_EXP: i64 = i32::MAX as i64;

/// Go's `int` of a length or a bit count: either is at most `isize::MAX`, so the saturation never fires.
fn go_int(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

// A u32 precision fits usize on every target this crate builds for, so `prec as usize` is lossless.
const _: () = assert!(usize::BITS >= u32::BITS);

/// The magnitude of a Go `int` shift, length or index whose sign the caller has checked, as usize: within the
/// exponent range it fits a 64-bit usize, and one past usize is refused.
fn go_uint(n: u64) -> Result<usize, TypesError> {
    usize::try_from(n)
        .map_err(|_| TypesError::big_float(format!("invariant violated: {n} exceeds usize")))
}

/// The value of a [`BigFloat`]: zero, infinity, or `mant × 2^exp` with `mant` odd.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Form {
    Zero,
    Finite { mant: UBig, exp: i64 },
    Inf,
}

/// Go's `big.Float`: a signed binary floating-point number with a precision in bits, rounded to nearest even.
///
/// Dgraph's bigfloat values are 200 bits ([`BIG_FLOAT_PRECISION`]); a datetime converted to a bigfloat is the
/// quotient of two 53-bit values and keeps Go's 53 bits.
///
/// [`BIG_FLOAT_PRECISION`]: crate::BIG_FLOAT_PRECISION
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BigFloat {
    prec: u32,
    neg: bool,
    form: Form,
}

impl BigFloat {
    /// Zero (`+0`) at a precision.
    pub fn zero(prec: u32) -> Self {
        Self {
            prec,
            neg: false,
            form: Form::Zero,
        }
    }

    /// Infinity of a sign at a precision.
    pub fn inf(neg: bool, prec: u32) -> Self {
        Self {
            prec,
            neg,
            form: Form::Inf,
        }
    }

    /// Go's `SetInt64` at a precision (exact for 64 bits and more); a precision of 0 is 64, as Go's is.
    pub fn from_i64(v: i64, prec: u32) -> Self {
        let prec = if prec == 0 { 64 } else { prec };
        let neg = v < 0;
        Self::from_parts(neg, UBig::from(v.unsigned_abs()), 0, prec)
    }

    /// Go's `SetFloat64` at a precision (exact for 53 bits and more); a precision of 0 is 53, as Go's is. `None`
    /// for NaN, where Go panics with `Float.SetFloat64(NaN)`.
    pub fn from_f64(v: f64, prec: u32) -> Option<Self> {
        if v.is_nan() {
            return None;
        }
        let prec = if prec == 0 { 53 } else { prec };
        let neg = v.is_sign_negative();
        if v.is_infinite() {
            return Some(Self::inf(neg, prec));
        }
        if v == 0.0 {
            return Some(Self {
                prec,
                neg,
                form: Form::Zero,
            });
        }
        let bits = v.to_bits();
        // The masked exponent field is 11 bits wide, so reading it as signed keeps its value.
        let biased = ((bits >> 52) & 0x7ff).cast_signed();
        let frac = bits & ((1 << 52) - 1);
        let (mant, exp) = if biased == 0 {
            (frac, -1074)
        } else {
            (frac | (1 << 52), biased - 1075)
        };
        Some(Self::from_parts(neg, UBig::from(mant), exp, prec))
    }

    /// `±mant × 2^exp` rounded to `prec` bits.
    pub(crate) fn from_parts(neg: bool, mant: UBig, exp: i64, prec: u32) -> Self {
        let mut x = Self {
            prec,
            neg,
            form: Form::Zero,
        };
        x.set_rounded(mant, exp, false);
        x
    }

    /// The precision in bits.
    pub fn prec(&self) -> u32 {
        self.prec
    }

    /// Whether the sign bit is set (`-0` and `-Inf` included).
    pub fn is_neg(&self) -> bool {
        self.neg
    }

    /// Whether the value is ±0.
    pub fn is_zero(&self) -> bool {
        self.form == Form::Zero
    }

    /// Whether the value is ±Inf.
    pub fn is_inf(&self) -> bool {
        self.form == Form::Inf
    }

    /// Sets `±mant × 2^exp + sticky` rounded to this precision, with Go's exponent range checks (`setExpAndRound`):
    /// an exponent below `MinExp` underflows to zero, above `MaxExp` overflows to infinity. A precision of 0 holds
    /// only zero, as Go's `SetPrec(0)` leaves it.
    pub(crate) fn set_rounded(&mut self, mant: UBig, exp: i64, sticky: bool) {
        if mant == UBig::ZERO || self.prec == 0 {
            self.form = Form::Zero;
            return;
        }
        let n = go_int(mant.bit_len());
        let go_exp = n + exp;
        if go_exp < MIN_EXP {
            self.form = Form::Zero;
            return;
        }
        if go_exp > MAX_EXP {
            self.form = Form::Inf;
            return;
        }
        let p = i64::from(self.prec);
        let (mut m, mut e) = (mant, exp);
        if n > p || sticky {
            // Go's `n - p` bits below the precision, none when the mantissa is shorter.
            let r = mant_bits_minus_prec(&m, self.prec);
            let rbit = r > 0 && m.bit(r - 1);
            let sbit = sticky || (r > 1 && (m.trailing_zeros().unwrap_or(0) < r - 1));
            m >>= r;
            e += go_int(r);
            if rbit && (sbit || m.bit(0)) {
                m += UBig::ONE;
                if go_int(m.bit_len()) > p {
                    if go_exp >= MAX_EXP {
                        self.form = Form::Inf;
                        return;
                    }
                    m >>= 1;
                    e += 1;
                }
            }
        }
        let tz = m.trailing_zeros().unwrap_or(0);
        m >>= tz;
        e += go_int(tz);
        self.form = Form::Finite { mant: m, exp: e };
    }

    /// The exponent of Go's representation `0.m × 2^e`.
    pub(crate) fn go_exp(&self) -> i64 {
        match &self.form {
            Form::Finite { mant, exp } => go_int(mant.bit_len()) + exp,
            Form::Zero | Form::Inf => 0,
        }
    }
}

/// The bits of `m` past `prec`, 0 when `m` holds no more than `prec` bits.
fn mant_bits_minus_prec(m: &UBig, prec: u32) -> usize {
    m.bit_len().saturating_sub(prec as usize)
}
