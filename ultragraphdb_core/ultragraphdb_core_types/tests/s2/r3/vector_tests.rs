/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported in part from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, r3/vector_test.go.
 * Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! R3Vector norms, normalization, products, angles, Ortho and Compare.

use core::f64::consts::PI;
use ultragraphdb_core_types::R3Vector;

fn v(x: f64, y: f64, z: f64) -> R3Vector {
    R3Vector::new(x, y, z)
}

fn eq(x: f64, y: f64) -> bool {
    (x - y).abs() < 1e-14
}

/// Go's r3.Vector.ApproxEqual.
fn approx(a: R3Vector, b: R3Vector) -> bool {
    (a.x() - b.x()).abs() < 1e-16 && (a.y() - b.y()).abs() < 1e-16 && (a.z() - b.z()).abs() < 1e-16
}

/// A vector whose squared norm underflows to zero.
#[allow(clippy::excessive_precision)]
const TINY: R3Vector = R3Vector::new(
    -0.0,
    4.3145006366056343748277397783556100978621924913975e-196,
    4.3145006366056343748277397783556100978621924913975e-196,
);

// Ported from golang/geo r3/vector_test.go, TestVectorNorm and TestVectorNorm2.
#[test]
fn r3_vector_norm() {
    let tests = [
        (v(0.0, 0.0, 0.0), 0.0, 0.0),
        (v(0.0, 1.0, 0.0), 1.0, 1.0),
        (v(1.0, 1.0, 1.0), 3f64.sqrt(), 3.0),
        (v(1.0, 2.0, 3.0), 14f64.sqrt(), 14.0),
        (v(3.0, -4.0, 12.0), 13.0, 169.0),
        (v(1.0, 1e-16, 1e-32), 1.0, 1.0),
        (TINY, 0.0, 0.0),
    ];
    for (have, norm, norm2) in tests {
        assert!(eq(have.norm(), norm), "{have:?}.norm() = {}", have.norm());
        assert!(
            eq(have.norm2(), norm2),
            "{have:?}.norm2() = {}",
            have.norm2()
        );
    }
}

// Ported from golang/geo r3/vector_test.go, TestVectorNormalize.
#[test]
fn r3_vector_normalize() {
    for have in [
        v(1.0, 0.0, 0.0),
        v(0.0, 1.0, 0.0),
        v(0.0, 0.0, 1.0),
        v(1.0, 1.0, 1.0),
        v(1.0, 1e-16, 1e-32),
        v(12.34, 56.78, 91.01),
    ] {
        let nv = have.normalize();
        assert!(
            eq(have.x() * nv.y(), have.y() * nv.x()),
            "{have:?} direction"
        );
        assert!(
            eq(have.x() * nv.z(), have.z() * nv.x()),
            "{have:?} direction"
        );
        assert!(eq(nv.norm(), 1.0), "|{nv:?}|");
    }
    let n = v(3.0, 0.0, -4.0).normalize();
    assert!(eq(n.x(), 0.6) && n.y() == 0.0 && eq(n.z(), -0.8), "{n:?}");
    // A vector whose squared norm underflows normalizes to zero rather than (NaN, +Inf, +Inf).
    assert_eq!(TINY.normalize(), v(0.0, 0.0, 0.0));
    assert_eq!(v(0.0, 0.0, 0.0).normalize(), v(0.0, 0.0, 0.0));
}

// Ported from golang/geo r3/vector_test.go, TestVectorIsUnit.
#[test]
fn r3_vector_is_unit() {
    const EPSILON: f64 = 1e-14;
    let tests = [
        (v(0.0, 0.0, 0.0), false),
        (v(0.0, 1.0, 0.0), true),
        (v(1.0 + 2.0 * EPSILON, 0.0, 0.0), true),
        (v(1.0 * (1.0 + EPSILON), 0.0, 0.0), true),
        (v(1.0, 1.0, 1.0), false),
        (v(1.0, 1e-16, 1e-32), true),
        // The bound is 5e-14 on the squared norm.
        (v(1.0 + 3e-14, 0.0, 0.0), false),
    ];
    for (have, want) in tests {
        assert_eq!(have.is_unit(), want, "{have:?}");
    }
}

// Ported from golang/geo r3/vector_test.go, TestVectorDot.
#[test]
fn r3_vector_dot() {
    let tests = [
        (v(1.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 1.0),
        (v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0), 0.0),
        (v(1.0, 0.0, 0.0), v(0.0, 1.0, 1.0), 0.0),
        (v(1.0, 1.0, 1.0), v(-1.0, -1.0, -1.0), -3.0),
        (v(1.0, 2.0, 2.0), v(-0.3, 0.4, -1.2), -1.9),
    ];
    for (a, b, want) in tests {
        assert!(eq(a.dot(b), want), "{a:?}·{b:?} = {}", a.dot(b));
        assert!(eq(b.dot(a), want), "{b:?}·{a:?} = {}", b.dot(a));
    }
}

// Ported from golang/geo r3/vector_test.go, TestVectorCross.
#[test]
fn r3_vector_cross() {
    let tests = [
        (v(1.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 0.0, 0.0)),
        (v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0), v(0.0, 0.0, 1.0)),
        (v(0.0, 1.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 0.0, -1.0)),
        (v(1.0, 2.0, 3.0), v(-4.0, 5.0, -6.0), v(-27.0, -6.0, 13.0)),
    ];
    for (a, b, want) in tests {
        assert!(approx(a.cross(b), want), "{a:?}×{b:?} = {:?}", a.cross(b));
    }
}

// Ported from golang/geo r3/vector_test.go, TestVectorAdd, TestVectorSub, TestVectorMul and the
// absolute value.
#[test]
fn r3_vector_add_sub_mul_abs() {
    let add = [
        (v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)),
        (v(1.0, 0.0, 0.0), v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0)),
        (v(1.0, 2.0, 3.0), v(4.0, 5.0, 7.0), v(5.0, 7.0, 10.0)),
        (v(1.0, -3.0, 5.0), v(1.0, -6.0, -6.0), v(2.0, -9.0, -1.0)),
    ];
    for (a, b, want) in add {
        assert_eq!(a.add(b), want, "{a:?}+{b:?}");
    }
    let sub = [
        (v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)),
        (v(1.0, 0.0, 0.0), v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0)),
        (v(1.0, 2.0, 3.0), v(4.0, 5.0, 7.0), v(-3.0, -3.0, -4.0)),
        (v(1.0, -3.0, 5.0), v(1.0, -6.0, -6.0), v(0.0, 3.0, 11.0)),
    ];
    for (a, b, want) in sub {
        assert_eq!(a.sub(b), want, "{a:?}-{b:?}");
    }
    let mul = [
        (v(0.0, 0.0, 0.0), 3.0, v(0.0, 0.0, 0.0)),
        (v(1.0, 0.0, 0.0), 1.0, v(1.0, 0.0, 0.0)),
        (v(1.0, 0.0, 0.0), 0.0, v(0.0, 0.0, 0.0)),
        (v(1.0, 0.0, 0.0), 3.0, v(3.0, 0.0, 0.0)),
        (v(1.0, -3.0, 5.0), -1.0, v(-1.0, 3.0, -5.0)),
        (v(1.0, -3.0, 5.0), 2.0, v(2.0, -6.0, 10.0)),
    ];
    for (a, m, want) in mul {
        assert_eq!(a.mul(m), want, "{m}*{a:?}");
    }
    assert_eq!(v(-1.0, 2.0, -0.0).abs(), v(1.0, 2.0, 0.0));
    assert_eq!(v(-1.0, 2.0, -3.0).abs().z().to_bits(), 3f64.to_bits());
}

// Ported from golang/geo r3/vector_test.go, TestVectorAngle.
#[test]
fn r3_vector_angle() {
    let tests = [
        (v(1.0, 0.0, 0.0), v(1.0, 0.0, 0.0), 0.0),
        (v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0), PI / 2.0),
        (v(1.0, 0.0, 0.0), v(0.0, 1.0, 1.0), PI / 2.0),
        (v(1.0, 0.0, 0.0), v(-1.0, 0.0, 0.0), PI),
        (v(1.0, 2.0, 3.0), v(2.0, 3.0, -1.0), 1.2055891055045298),
    ];
    for (a, b, want) in tests {
        assert!(eq(a.angle(b).radians(), want), "{a:?}.angle({b:?})");
        assert!(eq(b.angle(a).radians(), want), "{b:?}.angle({a:?})");
    }
}

// Ported from golang/geo r3/vector_test.go, TestVectorOrtho and TestVectorOrthoAlignment.
#[test]
fn r3_vector_ortho() {
    for have in [
        v(1.0, 0.0, 0.0),
        v(1.0, 1.0, 0.0),
        v(1.0, 2.0, 3.0),
        v(1.0, -2.0, -5.0),
        v(0.012, 0.0053, 0.00457),
        v(-0.012, -1.0, -0.00457),
    ] {
        assert!(eq(have.dot(have.ortho()), 0.0), "{have:?} not orthogonal");
        assert!(eq(have.ortho().norm(), 1.0), "|{have:?}.ortho()|");
    }
    let tests = [
        (v(1.0, 0.0, 0.0), v(0.0, -1.0, 0.0)),
        (v(0.0, 1.0, 0.0), v(0.0, 0.0, -1.0)),
        (v(0.0, 0.0, 1.0), v(-1.0, 0.0, 0.0)),
    ];
    for (have, want) in tests {
        assert_eq!(have.ortho(), want, "{have:?}");
    }
}

// Ported from golang/geo r3/vector_test.go, TestVectorIdentities.
#[test]
fn r3_vector_identities() {
    let tests = [
        (v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0)),
        (v(0.0, 0.0, 0.0), v(0.0, 1.0, 2.0)),
        (v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0)),
        (v(1.0, 0.0, 0.0), v(0.0, 1.0, 1.0)),
        (v(1.0, 1.0, 1.0), v(-1.0, -1.0, -1.0)),
        (v(1.0, 2.0, 2.0), v(-0.3, 0.4, -1.2)),
    ];
    for (a, b) in tests {
        let c1 = a.cross(b);
        let c2 = b.cross(a);
        assert!(
            eq(a.angle(b).radians(), b.angle(a).radians()),
            "angle commutes"
        );
        assert!(eq(a.dot(b), b.dot(a)), "dot commutes");
        assert!(approx(c1, c2.mul(-1.0)), "cross anti-commutes");
        assert!(
            eq(a.dot(c1), 0.0) && eq(b.dot(c1), 0.0),
            "cross is orthogonal"
        );
    }
}

// Ported from golang/geo r3/vector_test.go, TestVectorCmp.
#[test]
fn r3_vector_compare() {
    let tests = [
        (v(0.0, 0.0, 0.0), v(0.0, 0.0, 0.0), 0),
        (v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0), -1),
        (v(0.0, 1.0, 0.0), v(0.0, 0.0, 0.0), 1),
        (v(1.0, 2.0, 3.0), v(3.0, 2.0, 1.0), -1),
        (v(-1.0, 0.0, 0.0), v(0.0, 0.0, -1.0), -1),
        (v(8.0, 6.0, 4.0), v(7.0, 5.0, 3.0), 1),
        (v(-1.0, -0.5, 0.0), v(0.0, 0.0, 0.1), -1),
        (v(1.0, 2.0, 3.0), v(2.0, 3.0, 4.0), -1),
        (v(1.23, 4.56, 7.89), v(1.23, 4.56, 7.89), 0),
        // The last component decides when the first two are equal.
        (v(1.0, 2.0, 4.0), v(1.0, 2.0, 3.0), 1),
        (v(1.0, 2.0, 3.0), v(1.0, 2.0, 4.0), -1),
    ];
    for (a, b, want) in tests {
        assert_eq!(a.compare(b), want, "{a:?}.compare({b:?})");
    }
}
