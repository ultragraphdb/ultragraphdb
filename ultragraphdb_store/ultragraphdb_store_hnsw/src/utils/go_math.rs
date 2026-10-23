/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, math/pow.go (pow, for a positive finite base and a whole exponent), math/frexp.go (frexp)
 * and math/ldexp.go (ldexp). Changed for ultragraphdb: translated to Rust; only the paths Dgraph's getInsertLayer
 * reaches, math.Pow(1.0/5, n) for a whole n.
 * Copyright (c) 2026. Marvin Hansen.
 */

const MASK: u64 = 0x7FF;
const SHIFT: u32 = 52;
const BIAS: i64 = 1023;
const SMALLEST_NORMAL: f64 = 2.2250738585072014e-308;

/// Go's `math.Pow(x, n)` for a positive finite `x` and a whole `n`, by Go's repeated squaring of `x`'s fraction, so
/// the result is Go's bit for bit.
pub(crate) fn pow(x: f64, n: u32) -> f64 {
    if n == 0 || x == 1.0 {
        return 1.0;
    }
    if n == 1 {
        return x;
    }
    let (mut a1, mut ae) = (1.0f64, 0i64);
    let (mut x1, mut xe) = frexp(x);
    let mut i = n;
    while i != 0 {
        if !(-(1 << 12)..=(1 << 12)).contains(&xe) {
            ae += xe;
            break;
        }
        if i & 1 == 1 {
            a1 *= x1;
            ae += xe;
        }
        x1 *= x1;
        xe <<= 1;
        if x1 < 0.5 {
            x1 += x1;
            xe -= 1;
        }
        i >>= 1;
    }
    ldexp(a1, ae)
}

/// Go's `normalize`: a subnormal `x` scaled into the normal range, and the power of two taken out.
fn normalize(x: f64) -> (f64, i64) {
    if x.abs() < SMALLEST_NORMAL {
        (x * (1u64 << 52) as f64, -52)
    } else {
        (x, 0)
    }
}

/// Go's `math.Frexp` of a finite non-zero `f`: a fraction in [0.5, 1) and a power of two.
fn frexp(f: f64) -> (f64, i64) {
    if f == 0.0 || !f.is_finite() {
        return (f, 0);
    }
    let (f, mut exp) = normalize(f);
    let mut x = f.to_bits();
    exp += ((x >> SHIFT) & MASK) as i64 - BIAS + 1;
    x &= !(MASK << SHIFT);
    x |= ((-1 + BIAS) as u64) << SHIFT;
    (f64::from_bits(x), exp)
}

/// Go's `math.Ldexp`: `frac` times two to the `exp`, with Go's underflow, overflow and subnormal rounding.
fn ldexp(frac: f64, exp: i64) -> f64 {
    if frac == 0.0 || !frac.is_finite() {
        return frac;
    }
    let (frac, e) = normalize(frac);
    let mut exp = exp + e;
    let mut x = frac.to_bits();
    exp += ((x >> SHIFT) & MASK) as i64 - BIAS;
    if exp < -1075 {
        return 0.0f64.copysign(frac);
    }
    if exp > 1023 {
        return if frac < 0.0 {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let mut m = 1.0f64;
    if exp < -1022 {
        exp += 53;
        m = 1.0 / (1u64 << 53) as f64;
    }
    x &= !(MASK << SHIFT);
    x |= ((exp + BIAS) as u64) << SHIFT;
    m * f64::from_bits(x)
}
