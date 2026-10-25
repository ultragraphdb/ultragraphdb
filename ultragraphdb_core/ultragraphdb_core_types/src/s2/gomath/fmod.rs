/*
 * Copyright 2009-2010 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27.0, src/math/mod.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use super::frexp::frexp;
use super::ldexp::ldexp;

/// Go's `math.Mod`: the floating-point remainder of x/y, with the sign of x.
pub(crate) fn fmod(x: f64, y: f64) -> f64 {
    if y == 0.0 || x.is_infinite() || x.is_nan() || y.is_nan() {
        return super::NAN;
    }
    let y = y.abs();

    let (yfr, yexp) = frexp(y);
    let mut r = x;
    if x < 0.0 {
        r = -x;
    }

    while r >= y {
        let (rfr, mut rexp) = frexp(r);
        if rfr < yfr {
            rexp -= 1;
        }
        r -= ldexp(y, rexp - yexp);
    }
    if x < 0.0 {
        r = -r;
    }
    r
}
