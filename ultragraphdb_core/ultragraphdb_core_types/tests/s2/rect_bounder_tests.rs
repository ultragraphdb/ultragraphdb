/*
 * Copyright 2017 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported in part from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/rect_bounder_test.go.
 * Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! RectBounder's latitude bounds of an edge, and ExpandForSubregions.

use core::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};
use ultragraphdb_core_types::{
    Angle, LatLng, Point, R1Interval, R3Vector, Rect, RectBounder, S1Interval,
    expand_for_subregions,
};

const MACHINE_EPSILON: f64 = f64::EPSILON;
const RECT_ERROR_LAT: f64 = 10.0 * MACHINE_EPSILON;
const RECT_ERROR_LNG: f64 = MACHINE_EPSILON;

fn raw(x: f64, y: f64, z: f64) -> Point {
    Point::from_vector(R3Vector::new(x, y, z))
}

/// Go's test helper rectBoundForPoints, on unnormalized points as Go's test passes them.
fn bound_for(a: Point, b: Point) -> Rect {
    let mut bounder = RectBounder::new();
    bounder.add_point(a);
    bounder.add_point(b);
    bounder.rect_bound()
}

/// Go's test helper rectsApproxEqual.
fn approx(a: Rect, b: Rect, lat: f64, lng: f64) -> bool {
    (a.lat().lo() - b.lat().lo()).abs() < lat
        && (a.lat().hi() - b.lat().hi()).abs() < lat
        && (a.lng().lo() - b.lng().lo()).abs() < lng
        && (a.lng().hi() - b.lng().hi()).abs() < lng
}

#[test]
fn rect_bounder_of_nothing_is_empty() {
    assert!(RectBounder::new().rect_bound().is_empty());
    assert!(RectBounder::default().rect_bound().is_empty());
    // One point gives that point's rect, padded in latitude by 2 * DBL_EPSILON.
    let mut b = RectBounder::new();
    b.add_point(raw(1.0, 0.0, 0.0));
    let r = b.rect_bound();
    assert_eq!(
        (r.lat().lo(), r.lat().hi()),
        (-2.0 * MACHINE_EPSILON, 2.0 * MACHINE_EPSILON)
    );
    assert_eq!((r.lng().lo(), r.lng().hi()), (0.0, 0.0));
}

// Ported from golang/geo s2/rect_bounder_test.go, TestRectBounderMaxLatitudeSimple.
#[test]
fn rect_bounder_max_latitude_simple() {
    let cube_lat = (1.0 / 3f64.sqrt()).asin(); // 35.26 degrees
    let cube_lat_rect = Rect::new(
        R1Interval::from_point(-cube_lat).add_point(cube_lat),
        S1Interval::from_endpoints(-FRAC_PI_4, FRAC_PI_4),
    );
    let tests = [
        (raw(1.0, 1.0, 1.0), raw(1.0, -1.0, -1.0)),
        (raw(1.0, -1.0, 1.0), raw(1.0, 1.0, -1.0)),
    ];
    for (a, b) in tests {
        let got = bound_for(a, b);
        assert!(
            approx(got, cube_lat_rect, RECT_ERROR_LAT, RECT_ERROR_LNG),
            "({a:?}, {b:?}) = {got:?}"
        );
    }
}

// Ported from golang/geo s2/rect_bounder_test.go, TestRectBounderMaxLatitudeEdgeInterior.
#[test]
fn rect_bounder_max_latitude_edge_interior() {
    let tests = [
        // Max latitude, CW edge.
        (
            FRAC_PI_4 + 0.5 * RECT_ERROR_LAT,
            bound_for(raw(1.0, 1.0, 1.0), raw(1.0, -1.0, 1.0))
                .lat()
                .hi(),
        ),
        // Min latitude, CW edge.
        (
            -FRAC_PI_4 - 0.5 * RECT_ERROR_LAT,
            bound_for(raw(1.0, -1.0, -1.0), raw(-1.0, -1.0, -1.0))
                .lat()
                .lo(),
        ),
        // Max latitude, CCW edge.
        (
            FRAC_PI_4 + 0.5 * RECT_ERROR_LAT,
            bound_for(raw(1.0, -1.0, 1.0), raw(1.0, 1.0, 1.0))
                .lat()
                .hi(),
        ),
        // Min latitude, CCW edge.
        (
            -FRAC_PI_4 - 0.5 * RECT_ERROR_LAT,
            bound_for(raw(-1.0, 1.0, -1.0), raw(-1.0, -1.0, -1.0))
                .lat()
                .lo(),
        ),
        // Edges through one of the poles.
        (
            FRAC_PI_2,
            bound_for(raw(0.3, 0.4, 1.0), raw(-0.3, -0.4, 1.0))
                .lat()
                .hi(),
        ),
        (
            -FRAC_PI_2,
            bound_for(raw(0.3, 0.4, -1.0), raw(-0.3, -0.4, -1.0))
                .lat()
                .lo(),
        ),
    ];
    for (want, got) in tests {
        assert!((got - want).abs() <= 1e-15, "got {got}, want {want}");
    }
}

#[test]
fn rect_bounder_nearly_identical_or_antipodal_points() {
    // Antipodal points: the edge between them may go any way round the sphere.
    let x = Point::from_coords(1.0, 0.0, 0.0);
    let anti = Point::from_coords(-1.0, 0.0, 0.0);
    assert!(bound_for(x, anti).is_full());
    // Identical points: the bound of the point.
    let r = bound_for(x, x);
    assert_eq!((r.lng().lo(), r.lng().hi()), (0.0, 0.0));
    assert_eq!(r.lat().hi(), 2.0 * MACHINE_EPSILON);
    // Points on nearly opposite meridians: the longitude bound is full.
    let r = bound_for(
        Point::from_lat_lng(LatLng::from_degrees(10.0, 0.0)),
        Point::from_lat_lng(LatLng::from_degrees(10.0, 180.0)),
    );
    assert!(r.lng().is_full(), "{r:?}");
    assert_eq!(r.lat().hi(), FRAC_PI_2);
}

// Ported from golang/geo s2/rect_bounder_test.go, TestRectBounderExpandForSubregions.
#[test]
fn rect_bounder_expand_for_subregions() {
    assert!(expand_for_subregions(Rect::full()).is_full());
    assert!(expand_for_subregions(Rect::empty()).is_empty());

    let tests = [
        // The bound does not straddle the equator (but almost does), and spans nearly 180 degrees
        // in longitude.
        (3e-16, 0.0, 1e-14, PI, true),
        (9e-16, 0.0, 1e-14, PI, false),
        (1e-16, 7e-16, 1e-14, PI, true),
        (3e-16, 14e-16, 1e-14, PI, false),
        (1e-100, 14e-16, 1e-14, PI, true),
        (1e-100, 22e-16, 1e-14, PI, false),
        // The bound spans at most 90 degrees in longitude, and almost 180 degrees in latitude.
        (-FRAC_PI_2, -1e-15, FRAC_PI_2 - 7e-16, 0.0, true),
        (-FRAC_PI_2, -1e-15, FRAC_PI_2 - 30e-16, 0.0, false),
        (-FRAC_PI_2 + 4e-16, 0.0, FRAC_PI_2 - 2e-16, 1e-7, true),
        (-FRAC_PI_2 + 30e-16, 0.0, FRAC_PI_2, 1e-7, false),
        (-FRAC_PI_2 + 4e-16, 0.0, FRAC_PI_2 - 4e-16, FRAC_PI_2, true),
        (-FRAC_PI_2, 0.0, FRAC_PI_2 - 30e-16, FRAC_PI_2, false),
        // The bound straddles the equator and spans more than 90 degrees in longitude.
        (-FRAC_PI_2, 0.0, FRAC_PI_2 - 1e-8, PI - 1e-7, true),
        (-FRAC_PI_2, 0.0, FRAC_PI_2 - 1e-7, PI - 1e-7, false),
        (-FRAC_PI_2 + 1e-12, -PI + 1e-4, FRAC_PI_2, 0.0, true),
        (-FRAC_PI_2 + 1e-11, -PI + 1e-4, FRAC_PI_2, 0.0, true),
    ];
    for (x_lat, x_lng, y_lat, y_lng, want_full) in tests {
        let input = Rect::from_lat_lng(LatLng::new(
            Angle::from_radians(x_lat),
            Angle::from_radians(x_lng),
        ))
        .add_point(LatLng::new(
            Angle::from_radians(y_lat),
            Angle::from_radians(y_lng),
        ));
        let got = expand_for_subregions(input);
        let tag = format!("({x_lat}, {x_lng}, {y_lat}, {y_lng})");
        assert!(
            got.contains(input),
            "{tag} should contain the original rect"
        );
        assert_eq!(got.is_full(), want_full, "{tag} is_full");
    }

    let rect_tests = [
        (
            1.5,
            -FRAC_PI_2,
            1.5,
            FRAC_PI_2 - 2e-16,
            Rect::new(R1Interval::new(1.5, 1.5), S1Interval::full()),
        ),
        (
            1.5,
            -FRAC_PI_2,
            1.5,
            FRAC_PI_2 - 7e-16,
            Rect::new(
                R1Interval::new(1.5, 1.5),
                S1Interval::new(-FRAC_PI_2, FRAC_PI_2 - 7e-16),
            ),
        ),
        // The bound is expanded to include one of the poles.
        (
            -FRAC_PI_2 + 1e-15,
            0.0,
            -FRAC_PI_2 + 1e-15,
            0.0,
            Rect::new(
                R1Interval::new(-FRAC_PI_2, -FRAC_PI_2 + 1e-15),
                S1Interval::full(),
            ),
        ),
        (
            FRAC_PI_2 - 1e-15,
            0.0,
            FRAC_PI_2 - 1e-15,
            0.0,
            Rect::new(
                R1Interval::new(FRAC_PI_2 - 1e-15, FRAC_PI_2),
                S1Interval::full(),
            ),
        ),
    ];
    for (x_lat, x_lng, y_lat, y_lng, want) in rect_tests {
        let input = Rect::from_lat_lng(LatLng::new(
            Angle::from_radians(x_lat),
            Angle::from_radians(x_lng),
        ))
        .add_point(LatLng::new(
            Angle::from_radians(y_lat),
            Angle::from_radians(y_lng),
        ));
        let got = expand_for_subregions(input);
        assert!(
            approx(got, want, RECT_ERROR_LAT, RECT_ERROR_LNG),
            "({x_lat}, {x_lng}, {y_lat}, {y_lng}) = {got:?}, want {want:?}"
        );
    }
}
