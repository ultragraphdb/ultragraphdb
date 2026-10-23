/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26.4, src/math/ldexp.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use super::bits::{BIAS, MASK, SHIFT, normalize};

/// Go's `math.Ldexp`: the inverse of Frexp, frac × 2**exp.
pub(crate) fn ldexp(frac: f64, exp: i64) -> f64 {
    // special cases
    if frac == 0.0 {
        return frac; // correctly return -0
    }
    if frac.is_infinite() || frac.is_nan() {
        return frac;
    }
    let (frac, e) = normalize(frac);
    let mut exp = exp + e;
    let mut x = frac.to_bits();
    exp += ((x >> SHIFT) & MASK) as i64 - BIAS;
    if exp < -1075 {
        return 0f64.copysign(frac); // underflow
    }
    if exp > 1023 {
        // overflow
        if frac < 0.0 {
            return f64::NEG_INFINITY;
        }
        return f64::INFINITY;
    }
    let mut m: f64 = 1.0;
    if exp < -1022 {
        // denormal
        exp += 53;
        m = 1.0 / (1u64 << 53) as f64; // 2**-53
    }
    x &= !(MASK << SHIFT);
    x |= ((exp + BIAS) as u64) << SHIFT;
    m * f64::from_bits(x)
}
