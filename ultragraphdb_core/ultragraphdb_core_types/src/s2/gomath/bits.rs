/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26.4, src/math/bits.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

pub(crate) const MASK: u64 = 0x7FF;
pub(crate) const SHIFT: u32 = 64 - 11 - 1;
pub(crate) const BIAS: i64 = 1023;

/// Go's `math.normalize`: returns a normal number y and exponent exp satisfying x == y × 2**exp.
/// It assumes x is finite and non-zero.
pub(crate) fn normalize(x: f64) -> (f64, i64) {
    const SMALLEST_NORMAL: f64 = f64::from_bits(0x0010000000000000); // 2**-1022
    if x.abs() < SMALLEST_NORMAL {
        return (x * (1u64 << 52) as f64, -52);
    }
    (x, 0)
}
