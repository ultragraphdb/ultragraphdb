/*
 * Copyright 2010 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27.0, src/math/nextafter.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// Go's `math.Nextafter`: the next representable float64 value after x towards y.
pub(crate) fn nextafter(x: f64, y: f64) -> f64 {
    if x.is_nan() || y.is_nan() {
        super::NAN
    } else if x == y {
        x
    } else if x == 0.0 {
        f64::from_bits(1).copysign(y)
    } else if (y > x) == (x > 0.0) {
        f64::from_bits(x.to_bits() + 1)
    } else {
        f64::from_bits(x.to_bits() - 1)
    }
}
