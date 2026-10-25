/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported in part from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s1/angle_test.go.
 * Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Angle conversions between radians and degrees.

use core::f64::consts::PI;
use ultragraphdb_core_types::Angle;

// Ported from golang/geo s1/angle_test.go, TestEmptyValue.
#[test]
fn angle_empty_value_is_zero() {
    assert_eq!(Angle::default().radians(), 0.0);
}

// Ported from golang/geo s1/angle_test.go, TestPiRadiansExactly180Degrees.
#[test]
fn angle_pi_radians_is_exactly_180_degrees() {
    assert_eq!(Angle::from_radians(PI).radians(), PI);
    assert_eq!(Angle::from_radians(PI).degrees(), 180.0);
    assert_eq!(Angle::from_degrees(180.0).radians(), PI);
    assert_eq!(Angle::from_degrees(180.0).degrees(), 180.0);
    assert_eq!(Angle::from_radians(PI / 2.0).degrees(), 90.0);
    // Negative angles.
    assert_eq!(Angle::from_radians(-PI / 2.0).degrees(), -90.0);
    assert_eq!(Angle::from_degrees(-45.0).radians(), -PI / 4.0);
}

// Ported from golang/geo s1/angle_test.go, TestDegreesVsRadians: the exactness of specific values
// between degrees and radians.
#[test]
fn angle_degrees_vs_radians_are_exact() {
    for k in -8i32..=8 {
        let deg = f64::from(45 * k);
        assert_eq!(
            Angle::from_degrees(deg),
            Angle::from_radians((f64::from(k) * PI) / 4.0),
            "45°*{k}"
        );
        assert_eq!(Angle::from_degrees(deg).degrees(), deg, "45°*{k} degrees");
    }
    for k in 0..30 {
        let n = f64::from(1u32 << k);
        for (deg, rad) in [
            (180.0, 1.0),
            (60.0, 3.0),
            (36.0, 5.0),
            (20.0, 9.0),
            (4.0, 45.0),
        ] {
            assert_eq!(
                Angle::from_degrees(deg / n),
                Angle::from_radians(PI / (rad * n)),
                "{deg}°/{n}"
            );
        }
    }
    // A non-identity: 60° does not round-trip exactly.
    assert!((Angle::from_degrees(60.0).degrees() - 60.0).abs() > 1e-15);
}

#[test]
fn angle_inf_and_abs() {
    assert_eq!(Angle::inf().radians(), f64::INFINITY);
    assert_eq!(Angle::inf().degrees(), f64::INFINITY);
    let tests = [
        (-1.5, 1.5),
        (1.5, 1.5),
        (0.0, 0.0),
        (-0.0, 0.0),
        (f64::NEG_INFINITY, f64::INFINITY),
    ];
    for (have, want) in tests {
        let got = Angle::from_radians(have).abs().radians();
        assert_eq!(got.to_bits(), want.to_bits(), "abs({have})");
    }
    assert!(Angle::from_radians(f64::NAN).abs().radians().is_nan());
}
