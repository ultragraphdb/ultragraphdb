/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Cap construction, radius, AddPoint/AddCap and CellUnionBound, against Go.

use super::cellid_tests::{CapJson, same_cap};
use super::golden::{Checker, f, ids, load, pt};
use core::f64::consts::PI;
use serde::Deserialize;
use ultragraphdb_core_types::{Angle, Cap, CellId, ChordAngle, LatLng, Point, R3Vector};

#[derive(Deserialize)]
struct Case {
    center: [String; 3],
    angle: String,
    cap: CapJson,
    cell_union_bound: Vec<String>,
    points: Vec<[String; 3]>,
    contains_point: Vec<bool>,
    add_point: Vec<CapJson>,
    other: CapJson,
    add_cap: CapJson,
    complement: CapJson,
}

#[test]
fn caps_match_go() {
    let cases: Vec<Case> = load("caps.json");
    let mut c = Checker::new("caps");
    for (n, k) in cases.iter().enumerate() {
        let cap = Cap::from_center_angle(pt(&k.center), Angle::from_radians(f(&k.angle)));
        c.check(same_cap(cap, &k.cap), || format!("cap {n}: {cap:?}"));
        c.check(ids(&cap.cell_union_bound()) == k.cell_union_bound, || {
            format!("cap {n}: cell union bound")
        });
        let mut acc = cap;
        for (i, p) in k.points.iter().enumerate() {
            let q = pt(p);
            c.check(cap.contains_point(q) == k.contains_point[i], || {
                format!("cap {n}: contains point {i}")
            });
            acc = acc.add_point(q);
            c.check(same_cap(acc, &k.add_point[i]), || {
                format!("cap {n}: add point {i}: {acc:?}")
            });
        }
        let other = Cap::from_center_chord_angle(
            pt(&k.other.center),
            ChordAngle::from_squared_length(f(&k.other.chord)),
        );
        c.check(same_cap(other, &k.other), || {
            format!("cap {n}: other {other:?}")
        });
        c.check(same_cap(cap.add_cap(other), &k.add_cap), || {
            format!("cap {n}: add cap {:?}", cap.add_cap(other))
        });
        c.check(same_cap(cap.complement(), &k.complement), || {
            format!("cap {n}: complement {:?}", cap.complement())
        });
    }
    c.finish();
}

// The fixtures of golang/geo s2/cap_test.go (v0.0.0-20260427214057-41a1a8c7eb2a); the concave cap,
// whose error bounds need ChordAngle.MaxPointError, is left out.
const TINY_RAD: f64 = 1e-10;

fn raw(x: f64, y: f64, z: f64) -> Point {
    Point::from_vector(R3Vector::new(x, y, z))
}

fn x_axis() -> Cap {
    Cap::from_point(raw(1.0, 0.0, 0.0))
}

fn y_axis() -> Cap {
    Cap::from_point(raw(0.0, 1.0, 0.0))
}

/// Go's CapFromCenterHeight(PointFromCoords(1, 0, 1), 1).
fn hemi() -> Cap {
    Cap::from_center_chord_angle(
        Point::from_coords(1.0, 0.0, 1.0),
        ChordAngle::from_squared_length(2.0),
    )
}

fn tiny() -> Cap {
    Cap::from_center_angle(
        Point::from_coords(1.0, 2.0, 3.0),
        Angle::from_radians(TINY_RAD),
    )
}

/// Go's Cap.ApproxEqual.
fn approx(c: Cap, o: Cap) -> bool {
    const EPSILON: f64 = 1e-14;
    let (r2, o2) = (c.chord_radius().length2(), o.chord_radius().length2());
    c.center().approx_equal(o.center()) && (r2 - o2).abs() <= EPSILON
        || c.is_empty() && o2 <= EPSILON
        || o.is_empty() && r2 <= EPSILON
        || c.is_full() && o2 >= 2.0 - EPSILON
        || o.is_full() && r2 >= 2.0 - EPSILON
}

// Ported from golang/geo s2/cap_test.go, TestCapBasicEmptyFullValid (IsEmpty and IsFull).
#[test]
fn cap_basic_empty_full() {
    let x_comp = x_axis().complement();
    let tests = [
        (Cap::empty(), true, false),
        (Cap::empty().complement(), false, true),
        (Cap::full(), false, true),
        (Cap::full().complement(), true, false),
        (x_comp, false, true),
        (x_comp.complement(), true, false),
        (tiny(), false, false),
        (hemi(), false, false),
    ];
    for (c, empty, full) in tests {
        assert_eq!(c.is_empty(), empty, "{c:?}");
        assert_eq!(c.is_full(), full, "{c:?}");
    }
    assert_eq!(Cap::empty().center(), raw(1.0, 0.0, 0.0));
    assert_eq!(Cap::empty().chord_radius(), ChordAngle::NEGATIVE);
    assert_eq!(Cap::full().center(), raw(1.0, 0.0, 0.0));
    assert_eq!(Cap::full().chord_radius(), ChordAngle::STRAIGHT);
}

// Ported from golang/geo s2/cap_test.go, TestCapCenterHeightRadius.
#[test]
fn cap_center_height_radius() {
    assert_eq!(Cap::full().height(), 2.0);
    assert_eq!(Cap::full().radius().degrees(), 180.0);
    assert_eq!(Cap::empty().height(), -0.5);
    assert_eq!(Cap::empty().radius().radians(), -1.0);
    assert_eq!(y_axis().height(), 0.0);
    assert_eq!(x_axis().height(), 0.0);
    assert_eq!(x_axis().radius().radians(), 0.0);
    let hc = Point::from_vector(hemi().center().vector().mul(-1.0));
    assert_eq!(hemi().complement().center(), hc);
    assert_eq!(hemi().height(), 1.0);
    assert_eq!(hemi().complement().height(), 1.0);
    assert!((hemi().radius().radians() - PI / 2.0).abs() <= 1e-15);
}

// Ported from golang/geo s2/cap_test.go, TestCapContainsPoint (without the concave cases).
#[test]
fn cap_contains_point() {
    let x_pt = raw(1.0, 0.0, 0.0);
    let x_comp = x_axis().complement();
    let tiny = tiny();
    let tangent = tiny
        .center()
        .vector()
        .cross(R3Vector::new(3.0, 2.0, 1.0))
        .normalize();
    let tests = [
        (x_axis(), x_pt, true),
        (x_axis(), raw(1.0, 1e-20, 0.0), false),
        (y_axis(), x_axis().center(), false),
        (x_comp, x_axis().center(), true),
        (x_comp.complement(), x_axis().center(), false),
        (
            tiny,
            Point::from_vector(tiny.center().vector().add(tangent.mul(TINY_RAD * 0.99))),
            true,
        ),
        (
            tiny,
            Point::from_vector(tiny.center().vector().add(tangent.mul(TINY_RAD * 1.01))),
            false,
        ),
        (hemi(), Point::from_coords(1.0, 0.0, -(1.0 - 1e-15)), true),
        (hemi(), x_pt, true),
        (hemi().complement(), x_pt, false),
        (Cap::empty(), x_pt, false),
        (Cap::full(), raw(-1.0, 0.0, 0.0), true),
    ];
    for (c, p, want) in tests {
        assert_eq!(c.contains_point(p), want, "{c:?}.contains_point({p:?})");
    }
}

// Ported from golang/geo s2/cap_test.go, TestCapAddPoint.
#[test]
fn cap_add_point() {
    let x_pt = raw(1.0, 0.0, 0.0);
    let y_pt = raw(0.0, 1.0, 0.0);
    let half = Cap::from_center_angle(x_pt, Angle::from_radians(PI / 2.0));
    let hemi_center = raw(1.0, 0.0, 1.0);
    let tests = [
        // A cap plus its center equals itself.
        (x_axis(), x_pt, x_axis()),
        (y_axis(), y_pt, y_axis()),
        // A cap plus the opposite point equals full.
        (x_axis(), raw(-1.0, 0.0, 0.0), Cap::full()),
        (y_axis(), raw(0.0, -1.0, 0.0), Cap::full()),
        // A cap plus an orthogonal axis equals a half cap.
        (x_axis(), raw(0.0, 0.0, 1.0), half),
        (x_axis(), raw(0.0, 0.0, -1.0), half),
        // The 45 degree angled hemisphere plus some points.
        (
            hemi(),
            Point::from_coords(0.0, 1.0, -1.0),
            Cap::from_center_angle(hemi_center, Angle::from_degrees(120.0)),
        ),
        (
            hemi(),
            Point::from_coords(0.0, -1.0, -1.0),
            Cap::from_center_angle(hemi_center, Angle::from_degrees(120.0)),
        ),
        (
            hemi(),
            Point::from_coords(-1.0, -1.0, -1.0),
            Cap::from_center_angle(
                hemi_center,
                Angle::from_radians((-(2.0f64 / 3.0).sqrt()).acos()),
            ),
        ),
        (hemi(), raw(0.0, 1.0, 1.0), hemi()),
        (hemi(), raw(1.0, 0.0, 0.0), hemi()),
    ];
    for (have, p, want) in tests {
        let got = have.add_point(p);
        assert!(
            approx(got, want),
            "{have:?}.add_point({p:?}) = {got:?}, want {want:?}"
        );
        assert!(got.contains_point(p), "{got:?} contains {p:?}");
    }
    // The empty cap plus a point is that point.
    assert_eq!(Cap::empty().add_point(y_pt), Cap::from_point(y_pt));
}

// Ported from golang/geo s2/cap_test.go, TestCapAddCap.
#[test]
fn cap_add_cap() {
    let x_pt = raw(1.0, 0.0, 0.0);
    let tests = [
        (Cap::empty(), Cap::empty(), Cap::empty()),
        (Cap::full(), Cap::full(), Cap::full()),
        (Cap::full(), Cap::empty(), Cap::full()),
        (Cap::empty(), Cap::full(), Cap::full()),
        (x_axis(), Cap::empty(), x_axis()),
        (Cap::empty(), x_axis(), x_axis()),
        (y_axis(), Cap::empty(), y_axis()),
        (Cap::empty(), y_axis(), y_axis()),
        // Two halves make a whole.
        (x_axis(), x_axis().complement(), Cap::full()),
        // Two zero-height orthogonal axis caps make a half cap.
        (
            x_axis(),
            y_axis(),
            Cap::from_center_angle(x_pt, Angle::from_radians(PI / 2.0)),
        ),
    ];
    for (have, other, want) in tests {
        let got = have.add_cap(other);
        assert!(
            approx(got, want),
            "{have:?}.add_cap({other:?}) = {got:?}, want {want:?}"
        );
    }
    // A cap that already contains the other is unchanged.
    assert_eq!(hemi().add_cap(x_axis()), hemi());
}

// Ported from golang/geo s2/cap_test.go, TestCapCellUnionBoundLevel1Radius: a cap whose radius is
// about the width of a level-1 cell is covered by three faces' level-0 vertex neighbours. Go's
// MinWidthMetric.Value(1) is 2*sqrt(2)/3 / 2.
#[test]
fn cap_cell_union_bound_level1_radius() {
    let radius = 2.0 * 2f64.sqrt() / 3.0 / 2.0;
    let c = Cap::from_center_angle(
        Point::from_coords(1.0, 1.0, 1.0),
        Angle::from_radians(radius),
    );
    let covering = c.cell_union_bound();
    // Go's CellUnionBound here: faces 2, 0 and 1, in that order.
    let want: Vec<CellId> = [2, 0, 1].into_iter().map(CellId::from_face).collect();
    assert_eq!(covering, want);
    // The full cap needs all six faces.
    let full: Vec<CellId> = (0..6).map(CellId::from_face).collect();
    assert_eq!(Cap::full().cell_union_bound(), full);
}

#[test]
fn cap_complement_of_a_point_cap_is_full_and_of_a_hemisphere_is_its_opposite() {
    let c = Cap::from_center_angle(
        Point::from_lat_lng(LatLng::from_degrees(10.0, 20.0)),
        Angle::from_degrees(30.0),
    );
    let comp = c.complement();
    assert_eq!(
        comp.center(),
        Point::from_vector(c.center().vector().mul(-1.0))
    );
    assert!((comp.radius().degrees() - 150.0).abs() <= 1e-12, "{comp:?}");
    assert_eq!(Cap::empty().complement(), Cap::full());
    assert_eq!(Cap::full().complement(), Cap::empty());
}
