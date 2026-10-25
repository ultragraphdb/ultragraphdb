/*
 * Copyright 2010 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27.0, src/math/remainder.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The original C code is from FreeBSD's /usr/src/lib/msun/src/e_remainder.c
//! (Copyright (C) 1993 by Sun Microsystems, Inc.), as cited by Go.

use super::fmod::fmod;

/// Go's `math.Remainder`: the IEEE 754 floating-point remainder of x/y.
pub(crate) fn remainder(x: f64, y: f64) -> f64 {
    const TINY: f64 = f64::from_bits(0x0020000000000000); // 4.45014771701440276618e-308
    const HALF_MAX: f64 = f64::MAX / 2.0;
    // special cases
    if x.is_nan() || y.is_nan() || x.is_infinite() || y == 0.0 {
        return super::NAN;
    }
    if y.is_infinite() {
        return x;
    }
    let mut x = x;
    let mut y = y;
    let mut sign = false;
    if x < 0.0 {
        x = -x;
        sign = true;
    }
    if y < 0.0 {
        y = -y;
    }
    if x == y {
        if sign {
            return -0.0;
        }
        return 0.0;
    }
    if y <= HALF_MAX {
        x = fmod(x, y + y); // now x < 2y
    }
    if y < TINY {
        if x + x > y {
            x -= y;
            if x + x >= y {
                x -= y;
            }
        }
    } else {
        let y_half = 0.5 * y;
        if x > y_half {
            x -= y;
            if x >= y_half {
                x -= y;
            }
        }
    }
    if sign {
        x = -x;
    }
    x
}
