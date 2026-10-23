/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! An exact binary floating-point number: `mantissa * 2^exponent` with an unbounded integer
//! mantissa. golang/geo uses `big.Float` with a precision of 2^26 bits or more for its exact
//! predicates, which never rounds the sums and products of float64 values it computes; this
//! type gives the same exact results for exactly those operations.

use core::ops::{Add, Mul, Sub};
use dashu_int::{IBig, Sign};

#[derive(Clone, Debug)]
pub(crate) struct ExactFloat {
    mantissa: IBig,
    exponent: i64,
}

impl ExactFloat {
    /// The exact value of a finite float64; `None` for NaN and the infinities, on which Go's `big.Float` panics or
    /// which its arithmetic turns into a panicking NaN.
    pub(crate) fn from_f64(x: f64) -> Option<ExactFloat> {
        if !x.is_finite() {
            return None;
        }
        let bits = x.to_bits();
        let negative = bits >> 63 != 0;
        let biased = ((bits >> 52) & 0x7ff) as i64;
        let fraction = bits & ((1u64 << 52) - 1);
        let (m, e) = if biased == 0 {
            (fraction, -1074)
        } else {
            (fraction | (1u64 << 52), biased - 1075)
        };
        let m = IBig::from(m);
        Some(ExactFloat {
            mantissa: if negative { -m } else { m },
            exponent: e,
        })
    }

    /// The sign: -1, 0 or +1.
    pub(crate) fn sign(&self) -> i32 {
        if self.mantissa.is_zero() {
            return 0;
        }
        match self.mantissa.sign() {
            Sign::Positive => 1,
            Sign::Negative => -1,
        }
    }

    fn aligned(&self, exponent: i64) -> IBig {
        let shift = (self.exponent - exponent) as usize;
        &self.mantissa << shift
    }
}

impl Add for &ExactFloat {
    type Output = ExactFloat;

    fn add(self, rhs: &ExactFloat) -> ExactFloat {
        let e = self.exponent.min(rhs.exponent);
        ExactFloat {
            mantissa: self.aligned(e) + rhs.aligned(e),
            exponent: e,
        }
    }
}

impl Sub for &ExactFloat {
    type Output = ExactFloat;

    fn sub(self, rhs: &ExactFloat) -> ExactFloat {
        let e = self.exponent.min(rhs.exponent);
        ExactFloat {
            mantissa: self.aligned(e) - rhs.aligned(e),
            exponent: e,
        }
    }
}

impl Mul for &ExactFloat {
    type Output = ExactFloat;

    fn mul(self, rhs: &ExactFloat) -> ExactFloat {
        ExactFloat {
            mantissa: &self.mantissa * &rhs.mantissa,
            exponent: self.exponent + rhs.exponent,
        }
    }
}
