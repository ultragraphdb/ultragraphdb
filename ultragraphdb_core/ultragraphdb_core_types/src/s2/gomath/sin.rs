/*
 * Copyright 2011 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27.0, src/math/sin.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Floating-point sine and cosine, as Go computes them.
//!
//! Coefficients are #2633 from Hart & Cheney (18.80D), as in Go's `math/sin.go`, which in turn
//! follows the Cephes Math Library Release 2.8 (Copyright 1985, 1995, 2000 by Stephen L. Moshier).

use super::trig_reduce::{REDUCE_THRESHOLD, trig_reduce};

// 1.58962301576546568060e-10, -2.50507477628578072866e-8, 2.75573136213857245213e-6,
// -1.98412698295895385996e-4, 8.33333333332211858878e-3, -1.66666666666666307295e-1
const SIN: [f64; 6] = [
    f64::from_bits(0x3de5d8fd1fd19ccd),
    f64::from_bits(0xbe5ae5e5a9291f5d),
    f64::from_bits(0x3ec71de3567d48a1),
    f64::from_bits(0xbf2a01a019bfdf03),
    f64::from_bits(0x3f8111111110f7d0),
    f64::from_bits(0xbfc5555555555548),
];

// -1.13585365213876817300e-11, 2.08757008419747316778e-9, -2.75573141792967388112e-7,
// 2.48015872888517045348e-5, -1.38888888888730564116e-3, 4.16666666666665929218e-2
const COS: [f64; 6] = [
    f64::from_bits(0xbda8fa49a0861a9b),
    f64::from_bits(0x3e21ee9d7b4e3f05),
    f64::from_bits(0xbe927e4f7eac4bc6),
    f64::from_bits(0x3efa01a019c844f5),
    f64::from_bits(0xbf56c16c16c14f91),
    f64::from_bits(0x3fa555555555554b),
];

// Pi/4 split into three parts.
const PI4A: f64 = f64::from_bits(0x3fe921fb40000000);
const PI4B: f64 = f64::from_bits(0x3e64442d00000000);
const PI4C: f64 = f64::from_bits(0x3ce8469898cc5170);

// Go's constant `4 / Pi`, rounded once from the exact quotient.
const FOUR_OVER_PI: f64 = f64::from_bits(0x3ff45f306dc9c883);

/// Go's `math.Cos`.
pub(crate) fn cos(x: f64) -> f64 {
    if x.is_nan() || x.is_infinite() {
        return super::NAN;
    }

    let mut sign = false;
    let x = x.abs();

    let mut j: u64;
    let mut y: f64;
    let z: f64;
    if x >= REDUCE_THRESHOLD {
        (j, z) = trig_reduce(x);
    } else {
        j = (x * FOUR_OVER_PI) as u64; // integer part of x/(Pi/4), as integer for tests on the phase angle
        y = j as f64; // integer part of x/(Pi/4), as float

        if j & 1 == 1 {
            j += 1;
            y += 1.0;
        }
        j &= 7; // octant modulo 2Pi radians (360 degrees)
        z = ((x - y * PI4A) - y * PI4B) - y * PI4C; // Extended precision modular arithmetic
    }

    if j > 3 {
        j -= 4;
        sign = !sign;
    }
    if j > 1 {
        sign = !sign;
    }

    let zz = z * z;
    if j == 1 || j == 2 {
        y = z + z
            * zz
            * ((((((SIN[0] * zz) + SIN[1]) * zz + SIN[2]) * zz + SIN[3]) * zz + SIN[4]) * zz
                + SIN[5]);
    } else {
        y = 1.0 - 0.5 * zz
            + zz * zz
                * ((((((COS[0] * zz) + COS[1]) * zz + COS[2]) * zz + COS[3]) * zz + COS[4]) * zz
                    + COS[5]);
    }
    if sign {
        y = -y;
    }
    y
}

/// Go's `math.Sin`.
pub(crate) fn sin(x: f64) -> f64 {
    if x == 0.0 || x.is_nan() {
        return x; // return ±0 || NaN()
    }
    if x.is_infinite() {
        return super::NAN;
    }

    let mut sign = false;
    let mut x = x;
    if x < 0.0 {
        x = -x;
        sign = true;
    }

    let mut j: u64;
    let mut y: f64;
    let z: f64;
    if x >= REDUCE_THRESHOLD {
        (j, z) = trig_reduce(x);
    } else {
        j = (x * FOUR_OVER_PI) as u64; // integer part of x/(Pi/4), as integer for tests on the phase angle
        y = j as f64; // integer part of x/(Pi/4), as float

        if j & 1 == 1 {
            j += 1;
            y += 1.0;
        }
        j &= 7; // octant modulo 2Pi radians (360 degrees)
        z = ((x - y * PI4A) - y * PI4B) - y * PI4C; // Extended precision modular arithmetic
    }
    if j > 3 {
        sign = !sign;
        j -= 4;
    }
    let zz = z * z;
    if j == 1 || j == 2 {
        y = 1.0 - 0.5 * zz
            + zz * zz
                * ((((((COS[0] * zz) + COS[1]) * zz + COS[2]) * zz + COS[3]) * zz + COS[4]) * zz
                    + COS[5]);
    } else {
        y = z + z
            * zz
            * ((((((SIN[0] * zz) + SIN[1]) * zz + SIN[2]) * zz + SIN[3]) * zz + SIN[4]) * zz
                + SIN[5]);
    }
    if sign {
        y = -y;
    }
    y
}
