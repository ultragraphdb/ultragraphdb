/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported in part from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, r2/rect_test.go (the Point tests).
 * Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! R2Point arithmetic, Ortho, Dot and Cross.

use ultragraphdb_core_types::R2Point;

fn p(x: f64, y: f64) -> R2Point {
    R2Point::new(x, y)
}

// Ported from golang/geo r2/rect_test.go, TestOrtho.
#[test]
fn r2_point_ortho() {
    let s3 = 3f64.sqrt();
    let tests = [
        (p(0.0, 0.0), p(-0.0, 0.0)),
        (p(0.0, 1.0), p(-1.0, 0.0)),
        (p(1.0, 1.0), p(-1.0, 1.0)),
        (p(-4.0, 7.0), p(-7.0, -4.0)),
        (p(1.0, s3), p(-s3, 1.0)),
    ];
    for (have, want) in tests {
        assert_eq!(have.ortho(), want, "{have:?}");
    }
}

// Ported from golang/geo r2/rect_test.go, TestDot.
#[test]
fn r2_point_dot() {
    let tests = [
        (p(0.0, 0.0), p(0.0, 0.0), 0.0),
        (p(0.0, 1.0), p(0.0, 0.0), 0.0),
        (p(1.0, 1.0), p(4.0, 3.0), 7.0),
        (p(-4.0, 7.0), p(1.0, 5.0), 31.0),
    ];
    for (a, b, want) in tests {
        assert_eq!(a.dot(b), want, "{a:?}·{b:?}");
        assert_eq!(b.dot(a), want, "{b:?}·{a:?}");
    }
}

// Ported from golang/geo r2/rect_test.go, TestCross.
#[test]
fn r2_point_cross() {
    let tests = [
        (p(0.0, 0.0), p(0.0, 0.0), 0.0),
        (p(0.0, 1.0), p(0.0, 0.0), 0.0),
        (p(1.0, 1.0), p(-1.0, -1.0), 0.0),
        (p(1.0, 1.0), p(4.0, 3.0), -1.0),
        (p(1.0, 5.0), p(-2.0, 3.0), 13.0),
    ];
    for (a, b, want) in tests {
        assert_eq!(a.cross(b), want, "{a:?}×{b:?}");
        assert_eq!(b.cross(a), -want, "{b:?}×{a:?}");
    }
}

#[test]
fn r2_point_add_sub_mul() {
    let (a, b) = (p(1.0, -3.0), p(0.5, 4.0));
    assert_eq!(a.add(b), p(1.5, 1.0));
    assert_eq!(a.sub(b), p(0.5, -7.0));
    assert_eq!(b.sub(a), p(-0.5, 7.0));
    assert_eq!(a.mul(-2.0), p(-2.0, 6.0));
    assert_eq!(a.mul(0.0), p(0.0, -0.0));
    assert_eq!((a.x(), a.y()), (1.0, -3.0));
    assert_eq!(R2Point::default(), p(0.0, 0.0));
}
