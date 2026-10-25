/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27.0, src/math/asin.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use super::atan::satan;

// Go's constant `Pi / 2`.
const PI_2: f64 = f64::from_bits(0x3ff921fb54442d18);

/// Go's `math.Asin`, computed from the arctangent after range reduction.
pub(crate) fn asin(x: f64) -> f64 {
    if x == 0.0 {
        return x; // special case
    }
    let mut x = x;
    let mut sign = false;
    if x < 0.0 {
        x = -x;
        sign = true;
    }
    if x > 1.0 {
        return super::NAN; // special case
    }

    let mut temp = (1.0 - x * x).sqrt();
    if x > 0.7 {
        temp = PI_2 - satan(temp / x);
    } else {
        temp = satan(x / temp);
    }

    if sign {
        temp = -temp;
    }
    temp
}
