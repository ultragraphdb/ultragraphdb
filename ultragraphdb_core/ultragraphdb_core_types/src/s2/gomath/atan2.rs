/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26.4, src/math/atan2.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use super::atan::atan;

// Go's constants `Pi`, `Pi / 2`, `Pi / 4` and `3 * Pi / 4`.
const PI: f64 = f64::from_bits(0x400921fb54442d18);
const PI_2: f64 = f64::from_bits(0x3ff921fb54442d18);
const PI_4: f64 = f64::from_bits(0x3fe921fb54442d18);
const PI3_4: f64 = f64::from_bits(0x4002d97c7f3321d2);

/// Go's `math.Atan2`: the arc tangent of y/x, using the signs of the two to determine the
/// quadrant of the return value.
pub(crate) fn atan2(y: f64, x: f64) -> f64 {
    // special cases
    if y.is_nan() || x.is_nan() {
        return super::NAN;
    }
    if y == 0.0 {
        if x >= 0.0 && !x.is_sign_negative() {
            return 0f64.copysign(y);
        }
        return PI.copysign(y);
    }
    if x == 0.0 {
        return PI_2.copysign(y);
    }
    if x.is_infinite() {
        if x == f64::INFINITY {
            if y.is_infinite() {
                return PI_4.copysign(y);
            }
            return 0f64.copysign(y);
        }
        if y.is_infinite() {
            return PI3_4.copysign(y);
        }
        return PI.copysign(y);
    }
    if y.is_infinite() {
        return PI_2.copysign(y);
    }

    // Call atan and determine the quadrant.
    let q = atan(y / x);
    if x < 0.0 {
        if q <= 0.0 {
            return q + PI;
        }
        return q - PI;
    }
    q
}
