/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26.4, src/math/frexp.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use super::bits::{BIAS, MASK, SHIFT, normalize};

/// Go's `math.Frexp`: breaks f into a normalized fraction in [½, 1) and an integral power of two.
pub(crate) fn frexp(f: f64) -> (f64, i64) {
    // special cases
    if f == 0.0 {
        return (f, 0); // correctly return -0
    }
    if f.is_infinite() || f.is_nan() {
        return (f, 0);
    }
    let (f, mut exp) = normalize(f);
    let mut x = f.to_bits();
    exp += ((x >> SHIFT) & MASK) as i64 - BIAS + 1;
    x &= !(MASK << SHIFT);
    x |= ((-1 + BIAS) as u64) << SHIFT;
    (f64::from_bits(x), exp)
}
