/*
 * Copyright 2014 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26, src/math/big/float.go (Mul, Quo, Cmp, Float64, Int64). Changed for ultragraphdb: translated
 * to Rust onto dashu-int; each operation is the exact result rounded once, as Go's are.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::cmp::Ordering;

use dashu_base::{BitTest, DivRem, PowerOfTwo};
use dashu_int::UBig;

use crate::types::big_float::{BigFloat, Form};

impl BigFloat {
    /// Go's `z.SetPrec(prec).Mul(x, y)`: the product rounded to `prec` bits.
    pub(crate) fn mul(&self, y: &BigFloat, prec: u32) -> BigFloat {
        let neg = self.neg != y.neg;
        match (&self.form, &y.form) {
            (Form::Finite { mant: a, exp: ea }, Form::Finite { mant: b, exp: eb }) => {
                BigFloat::from_parts(neg, a * b, ea + eb, prec)
            }
            (Form::Inf, Form::Zero) | (Form::Zero, Form::Inf) => {
                panic!("big: multiplication of zero with infinity")
            }
            (Form::Inf, _) | (_, Form::Inf) => BigFloat::inf(neg, prec),
            _ => BigFloat {
                prec,
                neg,
                form: Form::Zero,
            },
        }
    }

    /// Go's `z.SetPrec(prec).Quo(x, y)`: the quotient rounded to `prec` bits.
    pub fn quo(&self, y: &BigFloat, prec: u32) -> BigFloat {
        let neg = self.neg != y.neg;
        match (&self.form, &y.form) {
            (Form::Finite { mant: a, exp: ea }, Form::Finite { mant: b, exp: eb }) => {
                // Shift the dividend so the quotient has at least prec + 2 bits; the remainder is the sticky bit.
                let shift =
                    (i64::from(prec) + 2 + b.bit_len() as i64 - a.bit_len() as i64).max(0) as usize;
                let (q, r) = (a << shift).div_rem(b);
                let mut z = BigFloat {
                    prec,
                    neg,
                    form: Form::Zero,
                };
                z.set_rounded(q, ea - eb - shift as i64, r != UBig::ZERO);
                z
            }
            (Form::Zero, Form::Zero) | (Form::Inf, Form::Inf) => {
                panic!("big: division of zero by zero or infinity by infinity")
            }
            (Form::Zero, _) | (_, Form::Inf) => BigFloat {
                prec,
                neg,
                form: Form::Zero,
            },
            _ => BigFloat::inf(neg, prec),
        }
    }

    /// Go's `Cmp`: -0 equals +0, infinities compare as such.
    pub fn compare(&self, y: &BigFloat) -> Ordering {
        let ord = |x: &BigFloat| -> i8 {
            match x.form {
                Form::Zero => 0,
                Form::Finite { .. } => {
                    if x.neg {
                        -1
                    } else {
                        1
                    }
                }
                Form::Inf => {
                    if x.neg {
                        -2
                    } else {
                        2
                    }
                }
            }
        };
        let (mx, my) = (ord(self), ord(y));
        if mx != my {
            return mx.cmp(&my);
        }
        if mx == 1 || mx == -1 {
            let mag = self.cmp_abs(y);
            return if mx == -1 { mag.reverse() } else { mag };
        }
        Ordering::Equal
    }

    fn cmp_abs(&self, y: &BigFloat) -> Ordering {
        let (Form::Finite { mant: a, exp: ea }, Form::Finite { mant: b, exp: eb }) =
            (&self.form, &y.form)
        else {
            unreachable!("finite operands")
        };
        let (ga, gb) = (self.go_exp(), y.go_exp());
        if ga != gb {
            return ga.cmp(&gb);
        }
        let e = (*ea).min(*eb);
        (a << (ea - e) as usize).cmp(&(b << (eb - e) as usize))
    }

    /// Go's `Float64`: the nearest float64, half to even, with Go's handling of denormals and overflow.
    pub fn to_f64(&self) -> f64 {
        let sign = if self.neg { -1.0 } else { 1.0 };
        let (mant, exp) = match &self.form {
            Form::Zero => return sign * 0.0,
            Form::Inf => return sign * f64::INFINITY,
            Form::Finite { mant, exp } => (mant, *exp),
        };
        let e = self.go_exp() - 1;
        let mut p: i64 = 53;
        if e < -1022 {
            p = 53 + 1022 + e;
            if p < 0 || p == 0 && mant.is_power_of_two() {
                return sign * 0.0;
            }
            if p == 0 {
                return sign * f64::from_bits(1);
            }
        }
        let r = BigFloat::from_parts(self.neg, mant.clone(), exp, p as u32);
        if r.go_exp() - 1 > 1023 {
            return sign * f64::INFINITY;
        }
        let (m, me) = match r.form {
            Form::Finite { mant, exp } => (mant, exp),
            _ => return sign * f64::INFINITY,
        };
        let m = u64::try_from(&m).expect("at most 53 bits");
        let n = 64 - i64::from(m.leading_zeros());
        let (m53, e53) = (m << (53 - n), me - (53 - n));
        let unbiased = e53 + 52;
        let bits = if unbiased >= -1022 {
            (((unbiased + 1023) as u64) << 52) | (m53 & ((1 << 52) - 1))
        } else {
            m << (me + 1074)
        };
        sign * f64::from_bits(bits)
    }

    /// Go's `Int64`: truncated toward zero, saturating at the int64 limits (infinities included).
    pub fn to_i64(&self) -> i64 {
        match &self.form {
            Form::Zero => 0,
            Form::Inf => {
                if self.neg {
                    i64::MIN
                } else {
                    i64::MAX
                }
            }
            Form::Finite { mant, exp } => {
                let ge = self.go_exp();
                if ge <= 0 {
                    return 0;
                }
                if ge <= 63 {
                    let i = if *exp >= 0 {
                        mant << *exp as usize
                    } else {
                        mant >> (-*exp) as usize
                    };
                    let i = u64::try_from(&i).expect("fits in 63 bits") as i64;
                    return if self.neg { -i } else { i };
                }
                if self.neg { i64::MIN } else { i64::MAX }
            }
        }
    }
}
