/*
 * Copyright 2010 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27.0, src/math/logb.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use super::bits::{BIAS, MASK, SHIFT, normalize};

/// Go's `math.Ilogb`: the binary exponent of x as an integer.
pub(crate) fn ilogb(x: f64) -> i64 {
    // special cases
    if x == 0.0 {
        return i64::from(i32::MIN);
    }
    if x.is_nan() || x.is_infinite() {
        return i64::from(i32::MAX);
    }
    let (x, exp) = normalize(x);
    // The masked exponent field is 11 bits wide, so reading it as signed keeps its value.
    ((x.to_bits() >> SHIFT) & MASK).cast_signed() - BIAS + exp
}
