/*
 * Copyright 2015 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported in part from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s1/chordangle_test.go.
 * Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! ChordAngle ordering, conversion to and from Angle, arithmetic and sines.

use core::f64::consts::PI;
use ultragraphdb_core_types::{Angle, ChordAngle};

/// s1's `epsilon`.
const EPSILON: f64 = 1e-15;

fn near(x: f64, y: f64, eps: f64) -> bool {
    (x - y).abs() <= eps
}

// Ported from golang/geo s1/chordangle_test.go, TestChordAngleBasics.
#[test]
fn chord_angle_basics() {
    let zero = ChordAngle::default();
    let neg = ChordAngle::NEGATIVE;
    let straight = ChordAngle::STRAIGHT;
    let inf = ChordAngle::inf();
    let tests = [
        (neg, neg, false, true),
        (neg, zero, true, false),
        (neg, straight, true, false),
        (neg, inf, true, false),
        (zero, zero, false, true),
        (zero, straight, true, false),
        (zero, inf, true, false),
        (straight, straight, false, true),
        (straight, inf, true, false),
        (inf, inf, false, true),
        (inf, straight, false, false),
    ];
    for (a, b, less, equal) in tests {
        assert_eq!(a < b, less, "{a:?} < {b:?}");
        assert_eq!(a == b, equal, "{a:?} == {b:?}");
    }
    assert_eq!(ChordAngle::NEGATIVE.length2(), -1.0);
    assert_eq!(ChordAngle::RIGHT.length2(), 2.0);
    assert_eq!(ChordAngle::STRAIGHT.length2(), 4.0);
}

// Ported from golang/geo s1/chordangle_test.go, TestChordAngleAngleEquality.
#[test]
fn chord_angle_angle_equality() {
    assert_eq!(ChordAngle::inf().angle(), Angle::inf());
    assert_eq!(ChordAngle::STRAIGHT.angle(), Angle::from_degrees(180.0));
    assert_eq!(ChordAngle::default().angle(), Angle::default());
    let d = ChordAngle::RIGHT.angle().degrees();
    assert!(near(90.0, d, 1e-13), "{d}");
    // A negative chord angle reads as -1 radian.
    assert_eq!(ChordAngle::NEGATIVE.angle().radians(), -1.0);
    assert_eq!(
        ChordAngle::from_squared_length(-0.5).angle().radians(),
        -1.0
    );
}

// Ported from golang/geo s1/chordangle_test.go, TestChordAngleIsFunctions.
#[test]
fn chord_angle_is_functions() {
    let tests = [
        (ChordAngle::default(), false, true, false),
        (ChordAngle::NEGATIVE, true, false, false),
        (ChordAngle::STRAIGHT, false, false, false),
        (ChordAngle::inf(), false, false, true),
    ];
    for (have, negative, zero, inf) in tests {
        assert_eq!(have.length2() < 0.0, negative, "{have:?}");
        assert_eq!(have.length2() == 0.0, zero, "{have:?}");
        assert_eq!(have.is_infinity(), inf, "{have:?}");
    }
}

// Ported from golang/geo s1/chordangle_test.go, TestChordAngleFromAngle.
#[test]
fn chord_angle_from_angle() {
    for angle in [0.0, 1.0, -1.0, PI] {
        let got = ChordAngle::from_angle(Angle::from_radians(angle))
            .angle()
            .radians();
        assert_eq!(got, angle, "{angle}");
    }
    assert_eq!(
        ChordAngle::from_angle(Angle::from_radians(PI)),
        ChordAngle::STRAIGHT
    );
    // Angles past π clamp to π, the largest finite chord angle.
    assert_eq!(
        ChordAngle::from_angle(Angle::from_radians(4.0)),
        ChordAngle::STRAIGHT
    );
    // Every negative angle gives the negative chord angle.
    assert_eq!(
        ChordAngle::from_angle(Angle::from_radians(-1e-300)),
        ChordAngle::NEGATIVE
    );
    assert_eq!(ChordAngle::from_angle(Angle::inf()), ChordAngle::inf());
    assert_eq!(ChordAngle::from_angle(Angle::inf()).angle(), Angle::inf());
    // A right angle: 2 * sin(π/4) squared.
    assert!(near(
        ChordAngle::from_angle(Angle::from_degrees(90.0)).length2(),
        2.0,
        EPSILON
    ));
}

#[test]
fn chord_angle_from_squared_length_clamps_to_straight() {
    let tests = [
        (0.0, 0.0),
        (1.5, 1.5),
        (4.0, 4.0),
        (f64::from_bits(4.0f64.to_bits() + 1), 4.0),
        (1e300, 4.0),
        (f64::INFINITY, 4.0),
        (-1.0, -1.0),
    ];
    for (have, want) in tests {
        assert_eq!(
            ChordAngle::from_squared_length(have).length2(),
            want,
            "{have}"
        );
    }
}

// Ported from golang/geo s1/chordangle_test.go, TestChordAngleArithmetic.
#[test]
fn chord_angle_arithmetic() {
    let zero = ChordAngle::default();
    let d30 = ChordAngle::from_angle(Angle::from_degrees(30.0));
    let d60 = ChordAngle::from_angle(Angle::from_degrees(60.0));
    let d90 = ChordAngle::from_angle(Angle::from_degrees(90.0));
    let d120 = ChordAngle::from_angle(Angle::from_degrees(120.0));
    let d180 = ChordAngle::STRAIGHT;
    let add_tests = [
        (zero, zero, zero),
        (d60, zero, d60),
        (zero, d60, d60),
        (d30, d60, d90),
        (d60, d30, d90),
        (d180, zero, d180),
        (d90, d90, d180),
        (d120, d90, d180),
        (d120, d120, d180),
        (d30, d180, d180),
        (d180, d180, d180),
    ];
    for (a, b, want) in add_tests {
        let got = a.add(b).length2();
        assert!(near(got, want.length2(), EPSILON), "{a:?} + {b:?} = {got}");
    }
    let sub_tests = [
        (zero, zero, zero),
        (d60, d60, zero),
        (d180, d180, zero),
        (zero, d60, zero),
        (d30, d90, zero),
        (d90, d30, d60),
        (d90, d60, d30),
        (d180, zero, d180),
    ];
    for (a, b, want) in sub_tests {
        let got = a.sub(b).length2();
        assert!(near(got, want.length2(), EPSILON), "{a:?} - {b:?} = {got}");
    }
}

// Ported from golang/geo s1/chordangle_test.go, TestChordAngleTrigonometry (the Sin part).
#[test]
fn chord_angle_trigonometry() {
    const ITERS: u32 = 40;
    for iter in 0..=ITERS {
        let radians = PI * f64::from(iter) / f64::from(ITERS);
        let angle = ChordAngle::from_angle(Angle::from_radians(radians));
        assert!(
            near(radians.sin(), angle.sin(), 1e-14),
            "({iter}/{ITERS})*π"
        );
        assert!(
            near(radians.sin().powi(2), angle.sin2(), 1e-14),
            "({iter}/{ITERS})*π sin2"
        );
    }
    // Unlike Angle, ChordAngle can represent 90 and 180 degrees exactly.
    let angle90 = ChordAngle::from_squared_length(2.0);
    let angle180 = ChordAngle::from_squared_length(4.0);
    assert_eq!(angle90.sin(), 1.0);
    assert_eq!(angle90.sin2(), 1.0);
    assert_eq!(angle180.sin(), 0.0);
    assert_eq!(angle180.sin2(), 0.0);
}

// Ported from golang/geo s1/chordangle_test.go, TestChordAngleExpanded.
#[test]
fn chord_angle_expanded() {
    let tests = [
        (ChordAngle::NEGATIVE, 5.0, ChordAngle::NEGATIVE),
        (ChordAngle::inf(), -5.0, ChordAngle::inf()),
        (ChordAngle::STRAIGHT, 5.0, ChordAngle::STRAIGHT),
        (ChordAngle::default(), -5.0, ChordAngle::default()),
        (
            ChordAngle::from_squared_length(1.25),
            0.25,
            ChordAngle::from_squared_length(1.5),
        ),
        (
            ChordAngle::from_squared_length(0.75),
            0.25,
            ChordAngle::from_squared_length(1.0),
        ),
    ];
    for (have, add, want) in tests {
        assert_eq!(have.expanded(add), want, "{have:?}.expanded({add})");
    }
}
