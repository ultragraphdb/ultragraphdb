/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported in part from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/point_test.go.
 * Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Point construction, PointCross, Distance, ApproxEqual and ChordAngleBetweenPoints.

use core::f64::consts::PI;
use ultragraphdb_core_types::{
    Angle, ChordAngle, LatLng, Point, R3Vector, chord_angle_between_points,
};

/// s2's test `epsilon`.
const EPSILON: f64 = 1e-15;

fn raw(x: f64, y: f64, z: f64) -> Point {
    Point::from_vector(R3Vector::new(x, y, z))
}

fn eq(x: f64, y: f64) -> bool {
    (x - y).abs() <= EPSILON
}

// Ported from golang/geo s2/point_test.go, TestOriginPoint.
#[test]
#[allow(clippy::excessive_precision)]
fn point_origin() {
    let o = Point::origin();
    assert!((o.vector().norm() - 1.0).abs() <= 1e-15);
    assert_eq!(
        (o.x(), o.y(), o.z()),
        (
            -0.0099994664350250197,
            0.0025924542609324121,
            0.99994664350250195
        )
    );
    // About 66km from the north pole towards the East Siberian Sea: stToUV(2/3) is 7/27.
    let p = raw(-0.01, 0.01 * (7.0 / 27.0), 1.0);
    assert!(p.approx_equal(o));
    // Not too close to either pole (6371.01 km is s2's earth radius).
    assert!(o.z().acos() * 6371.01 > 50.0);
}

#[test]
fn point_from_coords_normalizes_and_maps_zero_to_the_origin() {
    assert_eq!(Point::from_coords(0.0, 0.0, 0.0), Point::origin());
    assert_eq!(Point::from_coords(0.0, 0.0, -0.0), Point::origin());
    let p = Point::from_coords(0.0, 3.0, 4.0);
    assert!(eq(p.x(), 0.0) && eq(p.y(), 0.6) && eq(p.z(), 0.8), "{p:?}");
    assert_eq!(Point::from_coords(2.0, 0.0, 0.0), raw(1.0, 0.0, 0.0));
    // from_vector keeps the vector as given.
    assert_eq!(raw(2.0, 0.0, 0.0).vector(), R3Vector::new(2.0, 0.0, 0.0));
}

#[test]
fn point_from_lat_lng_axes() {
    let tests = [
        (0.0, 0.0, raw(1.0, 0.0, 0.0)),
        (0.0, 90.0, raw(0.0, 1.0, 0.0)),
        (90.0, 0.0, raw(0.0, 0.0, 1.0)),
        (-90.0, 0.0, raw(0.0, 0.0, -1.0)),
        (0.0, 180.0, raw(-1.0, 0.0, 0.0)),
        (0.0, -90.0, raw(0.0, -1.0, 0.0)),
    ];
    for (lat, lng, want) in tests {
        let p = Point::from_lat_lng(LatLng::from_degrees(lat, lng));
        assert!(
            eq(p.x(), want.x()) && eq(p.y(), want.y()) && eq(p.z(), want.z()),
            "({lat}, {lng}) = {p:?}"
        );
    }
}

// Ported from golang/geo s2/point_test.go, TestPointCross.
#[test]
fn point_cross() {
    let tests = [
        ([1.0, 0.0, 0.0], [1.0, 0.0, 0.0], 1.0),
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 2.0),
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], 2.0),
        ([1.0, 2.0, 3.0], [-4.0, 5.0, -6.0], 2.0 * 934f64.sqrt()),
    ];
    for (a, b, norm) in tests {
        let (p1, p2) = (raw(a[0], a[1], a[2]), raw(b[0], b[1], b[2]));
        let result = p1.point_cross(p2);
        assert!(
            (result.vector().norm() - norm).abs() <= 1e-14,
            "|{p1:?} × {p2:?}|"
        );
        assert!(eq(result.vector().dot(p1.vector()), 0.0));
        assert!(eq(result.vector().dot(p2.vector()), 0.0));
    }
    // Identical points have no cross product; the result is Ortho of the point.
    let x = raw(1.0, 0.0, 0.0);
    let got = x.point_cross(x);
    assert_eq!(got.vector(), R3Vector::new(0.0, -1.0, 0.0));
}

// Ported from golang/geo s2/point_test.go, TestPointDistance.
#[test]
fn point_distance() {
    let tests = [
        ([1.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.0),
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], PI / 2.0),
        ([1.0, 0.0, 0.0], [0.0, 1.0, 1.0], PI / 2.0),
        ([1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], PI),
        ([1.0, 2.0, 3.0], [2.0, 3.0, -1.0], 1.2055891055045298),
    ];
    for (a, b, want) in tests {
        let (p1, p2) = (raw(a[0], a[1], a[2]), raw(b[0], b[1], b[2]));
        assert!(
            eq(p1.distance(p2).radians(), want),
            "{p1:?}.distance({p2:?})"
        );
        assert!(
            eq(p2.distance(p1).radians(), want),
            "{p2:?}.distance({p1:?})"
        );
    }
}

// Ported from golang/geo s2/point_test.go, TestChordAngleBetweenPoints, on fixed frames rather
// than random ones.
#[test]
fn point_chord_angle_between_points() {
    let s = 0.5f64.sqrt();
    let frames = [
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([s, s, 0.0], [-s, s, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, s, -s], [0.0, s, s], [1.0, 0.0, 0.0]),
    ];
    for (x, y, z) in frames {
        let (x, y, z) = (
            raw(x[0], x[1], x[2]),
            raw(y[0], y[1], y[2]),
            raw(z[0], z[1], z[2]),
        );
        assert_eq!(
            chord_angle_between_points(z, z).angle(),
            Angle::from_radians(0.0)
        );
        let anti = Point::from_vector(z.vector().mul(-1.0));
        assert!((chord_angle_between_points(anti, z).angle().radians() - PI).abs() <= 1e-7);
        assert_eq!(chord_angle_between_points(anti, z), ChordAngle::STRAIGHT);
        assert!(eq(
            chord_angle_between_points(x, z).angle().radians(),
            PI / 2.0
        ));
        let w = Point::from_vector(y.vector().add(z.vector()).normalize());
        assert!(eq(
            chord_angle_between_points(w, z).angle().radians(),
            PI / 4.0
        ));
    }
    // The squared distance is clamped to 4, the straight angle.
    let big = chord_angle_between_points(raw(2.0, 0.0, 0.0), raw(-2.0, 0.0, 0.0));
    assert_eq!(big.length2(), 4.0);
}

// Ported from golang/geo s2/point_test.go, TestPointApproxEqual (with s2's epsilon 1e-15).
#[test]
fn point_approx_equal() {
    let e = EPSILON;
    let tests = [
        ([1.0, 0.0, 0.0], [1.0, 0.0, 0.0], true),
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], false),
        ([1.0, 0.0, 0.0], [0.0, 1.0, 1.0], false),
        ([1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], false),
        ([1.0, 2.0, 3.0], [2.0, 3.0, -1.0], false),
        ([1.0, 0.0, 0.0], [1.0 * (1.0 + e), 0.0, 0.0], true),
        ([1.0, 0.0, 0.0], [1.0 * (1.0 - e), 0.0, 0.0], true),
        ([1.0, 0.0, 0.0], [1.0 + e, 0.0, 0.0], true),
        ([1.0, 0.0, 0.0], [1.0 - e, 0.0, 0.0], true),
        ([1.0, 0.0, 0.0], [1.0, e, 0.0], true),
        ([1.0, 0.0, 0.0], [1.0, e, e], false),
        ([1.0, e, 0.0], [1.0, -e, e], false),
    ];
    for (a, b, want) in tests {
        let (p1, p2) = (raw(a[0], a[1], a[2]), raw(b[0], b[1], b[2]));
        assert_eq!(p1.approx_equal(p2), want, "{p1:?}.approx_equal({p2:?})");
    }
}
