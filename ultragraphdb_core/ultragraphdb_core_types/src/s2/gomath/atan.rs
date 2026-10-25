/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27.0, src/math/atan.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Floating-point arctangent, as Go computes it (Cephes Math Library Release 2.8,
//! Copyright 1984, 1987, 1989, 1992, 2000 by Stephen L. Moshier).

// Go's constants `Pi / 2` and `Pi / 4`.
const PI_2: f64 = f64::from_bits(0x3ff921fb54442d18);
const PI_4: f64 = f64::from_bits(0x3fe921fb54442d18);

/// Evaluates a series valid in the range [0, 0.66].
fn xatan(x: f64) -> f64 {
    const P0: f64 = f64::from_bits(0xbfec007fa1f72594); // -8.750608600031904122785e-01
    const P1: f64 = f64::from_bits(0xc03028545b6b807a); // -1.615753718733365076637e+01
    const P2: f64 = f64::from_bits(0xc052c08c36880273); // -7.500855792314704667340e+01
    const P3: f64 = f64::from_bits(0xc05eb8bf2d05ba25); // -1.228866684490136173410e+02
    const P4: f64 = f64::from_bits(0xc0503669fd28ec8e); // -6.485021904942025371773e+01
    const Q0: f64 = f64::from_bits(0x4038dbc45b14603c); // +2.485846490142306297962e+01
    const Q1: f64 = f64::from_bits(0x4064a0dd43b8fa25); // +1.650270098316988542046e+02
    const Q2: f64 = f64::from_bits(0x407b0e18d2e2be3b); // +4.328810604912902668951e+02
    const Q3: f64 = f64::from_bits(0x407e563f13b049ea); // +4.853903996359136964868e+02
    const Q4: f64 = f64::from_bits(0x4068519efbbd62ec); // +1.945506571482613964425e+02
    let mut z = x * x;
    z = z * ((((P0 * z + P1) * z + P2) * z + P3) * z + P4)
        / (((((z + Q0) * z + Q1) * z + Q2) * z + Q3) * z + Q4);
    x * z + x
}

/// Reduces its argument (known to be positive) to the range [0, 0.66] and calls xatan.
pub(crate) fn satan(x: f64) -> f64 {
    const MOREBITS: f64 = f64::from_bits(0x3c91a62633145c07); // pi/2 = PIO2 + Morebits
    const HALF_MOREBITS: f64 = f64::from_bits(0x3c81a62633145c07); // 0.5 * Morebits
    const TAN3PIO8: f64 = f64::from_bits(0x4003504f333f9de6); // tan(3*pi/8)
    if x <= 0.66 {
        return xatan(x);
    }
    if x > TAN3PIO8 {
        return PI_2 - xatan(1.0 / x) + MOREBITS;
    }
    PI_4 + xatan((x - 1.0) / (x + 1.0)) + HALF_MOREBITS
}

/// Go's `math.Atan`.
pub(crate) fn atan(x: f64) -> f64 {
    if x == 0.0 {
        return x;
    }
    if x > 0.0 {
        return satan(x);
    }
    -satan(-x)
}
