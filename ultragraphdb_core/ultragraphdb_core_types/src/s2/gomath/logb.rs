/*
 * Copyright 2010 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26.4, src/math/logb.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use super::bits::{BIAS, MASK, SHIFT, normalize};

/// Go's `math.Ilogb`: the binary exponent of x as an integer.
pub(crate) fn ilogb(x: f64) -> i64 {
    // special cases
    if x == 0.0 {
        return i32::MIN as i64;
    }
    if x.is_nan() || x.is_infinite() {
        return i32::MAX as i64;
    }
    let (x, exp) = normalize(x);
    ((x.to_bits() >> SHIFT) & MASK) as i64 - BIAS + exp
}
