/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Rect operations on pairs of loop bounds and ExpandForSubregions, against Go.

use super::cellid_tests::{CapJson, same_cap};
use super::golden::{Checker, ids, load, pt, rect, same_rect};
use core::f64::consts::{FRAC_PI_2, PI};
use serde::Deserialize;
use ultragraphdb_core_types::{
    Angle, Cap, LatLng, Point, R1Interval, R3Vector, Rect, S1Interval, expand_for_subregions,
};

#[derive(Deserialize)]
struct Pair {
    a: [String; 4],
    b: [String; 4],
    intersects: bool,
    contains: bool,
    union: [String; 4],
    union_is_full: bool,
}

#[derive(Deserialize)]
struct Single {
    r: [String; 4],
    is_empty: bool,
    is_full: bool,
    cap_bound: CapJson,
    expand_for_subregions: [String; 4],
    points: Vec<[String; 3]>,
    contains_point: Vec<bool>,
    cell_union_bound: Vec<String>,
}

#[derive(Deserialize)]
struct Golden {
    pairs: Vec<Pair>,
    singles: Vec<Single>,
}

#[test]
fn rects_match_go() {
    let g: Golden = load("rects.json");
    let mut c = Checker::new("rects");
    for k in &g.pairs {
        let (a, b) = (rect(&k.a), rect(&k.b));
        c.check(a.intersects(b) == k.intersects, || {
            format!("{a:?}.intersects({b:?})")
        });
        c.check(a.contains(b) == k.contains, || {
            format!("{a:?}.contains({b:?})")
        });
        let u = a.union(b);
        c.check(same_rect(u, &k.union), || {
            format!("{a:?}.union({b:?}) = {u:?}")
        });
        c.check(u.is_full() == k.union_is_full, || {
            format!("{u:?}.is_full()")
        });
    }
    for k in &g.singles {
        let r = rect(&k.r);
        c.check(
            r.is_empty() == k.is_empty && r.is_full() == k.is_full,
            || format!("{r:?} empty/full"),
        );
        c.check(same_cap(r.cap_bound(), &k.cap_bound), || {
            format!("{r:?} cap bound {:?}", r.cap_bound())
        });
        let e = expand_for_subregions(r);
        c.check(same_rect(e, &k.expand_for_subregions), || {
            format!("{r:?} expand_for_subregions = {e:?}")
        });
        for (p, want) in k.points.iter().zip(&k.contains_point) {
            c.check(r.contains_point(pt(p)) == *want, || {
                format!("{r:?} contains_point({p:?})")
            });
        }
        c.check(ids(&r.cell_union_bound()) == k.cell_union_bound, || {
            format!("{r:?} cell union bound")
        });
    }
    c.finish();
}

/// Go's test helper rectFromDegrees.
fn deg(lat_lo: f64, lng_lo: f64, lat_hi: f64, lng_hi: f64) -> Rect {
    Rect::new(
        R1Interval::new(
            Angle::from_degrees(lat_lo).radians(),
            Angle::from_degrees(lat_hi).radians(),
        ),
        S1Interval::from_endpoints(
            Angle::from_degrees(lng_lo).radians(),
            Angle::from_degrees(lng_hi).radians(),
        ),
    )
}

/// Go's test helper rectsApproxEqual with s2's epsilon 1e-15.
fn near(a: Rect, b: Rect) -> bool {
    (a.lat().lo() - b.lat().lo()).abs() < 1e-15
        && (a.lat().hi() - b.lat().hi()).abs() < 1e-15
        && (a.lng().lo() - b.lng().lo()).abs() < 1e-15
        && (a.lng().hi() - b.lng().hi()).abs() < 1e-15
}

fn rad(lat: f64, lng: f64) -> LatLng {
    LatLng::new(Angle::from_radians(lat), Angle::from_radians(lng))
}

// Ported from golang/geo s2/rect_test.go, TestRectEmptyAndFull.
#[test]
fn rect_empty_and_full() {
    let tests = [
        (Rect::empty(), true, true, false),
        (Rect::full(), true, false, true),
    ];
    for (r, valid, empty, full) in tests {
        assert_eq!(r.is_valid(), valid, "{r:?}");
        assert_eq!(r.is_empty(), empty, "{r:?}");
        assert_eq!(r.is_full(), full, "{r:?}");
    }
    // Invalid rects: a latitude past the poles, an invalid longitude, and one empty interval
    // beside a non-empty one.
    for r in [
        Rect::new(
            R1Interval::new(0.0, FRAC_PI_2 + 1e-15),
            S1Interval::new(0.0, 1.0),
        ),
        Rect::new(R1Interval::new(0.0, 1.0), S1Interval::new(0.0, PI + 1e-15)),
        Rect::new(R1Interval::empty(), S1Interval::new(0.0, 1.0)),
        Rect::new(R1Interval::new(0.0, 1.0), S1Interval::empty()),
    ] {
        assert!(!r.is_valid(), "{r:?}");
    }
}

// Ported from golang/geo s2/rect_test.go, TestRectFromLatLng.
#[test]
fn rect_from_lat_lng() {
    let ll = LatLng::from_degrees(23.0, 47.0);
    let got = Rect::from_lat_lng(ll);
    assert_eq!(got.center(), ll);
    assert_eq!(got.lo(), ll);
    assert_eq!(got.hi(), ll);
    assert_eq!(got.rect_bound(), got);
}

// Ported from golang/geo s2/rect_test.go, TestRectAddPoint.
#[test]
fn rect_add_point() {
    let tests = [
        (
            Rect::empty(),
            LatLng::from_degrees(0.0, 0.0),
            deg(0.0, 0.0, 0.0, 0.0),
        ),
        (
            deg(0.0, 0.0, 0.0, 0.0),
            rad(0.0, -PI / 2.0),
            deg(0.0, -90.0, 0.0, 0.0),
        ),
        (
            deg(0.0, -90.0, 0.0, 0.0),
            rad(PI / 4.0, -PI),
            deg(0.0, -180.0, 45.0, 0.0),
        ),
        (
            deg(0.0, -180.0, 45.0, 0.0),
            rad(PI / 2.0, 0.0),
            deg(0.0, -180.0, 90.0, 0.0),
        ),
    ];
    for (input, ll, want) in tests {
        let got = input.add_point(ll);
        assert!(
            near(got, want),
            "{input:?}.add_point({ll:?}) = {got:?}, want {want:?}"
        );
    }
    // An invalid LatLng leaves the rect unchanged.
    let r = deg(0.0, 0.0, 10.0, 10.0);
    assert_eq!(r.add_point(LatLng::from_degrees(100.0, 0.0)), r);
    assert_eq!(r.add_point(LatLng::from_degrees(0.0, 200.0)), r);
}

// Ported from golang/geo s2/rect_test.go, TestRectContainsLatLng.
#[test]
fn rect_contains_lat_lng() {
    let r = deg(0.0, -180.0, 90.0, 0.0);
    let tests = [
        (LatLng::from_degrees(30.0, -45.0), true),
        (LatLng::from_degrees(30.0, 45.0), false),
        (LatLng::from_degrees(0.0, -180.0), true),
        (LatLng::from_degrees(90.0, 0.0), true),
        // An invalid LatLng is never contained.
        (LatLng::from_degrees(91.0, -45.0), false),
        (LatLng::from_degrees(30.0, -181.0), false),
    ];
    for (ll, want) in tests {
        assert_eq!(r.contains_lat_lng(ll), want, "{ll:?}");
    }
    assert!(!Rect::full().contains_lat_lng(LatLng::from_degrees(120.0, 200.0)));
}

// Ported from golang/geo s2/rect_test.go, TestRectPolarClosure.
#[test]
fn rect_polar_closure() {
    let tests = [
        (deg(-89.0, 0.0, 89.0, 1.0), deg(-89.0, 0.0, 89.0, 1.0)),
        (
            deg(-90.0, -30.0, -45.0, 100.0),
            deg(-90.0, -180.0, -45.0, 180.0),
        ),
        (
            deg(89.0, 145.0, 90.0, 146.0),
            deg(89.0, -180.0, 90.0, 180.0),
        ),
        (deg(-90.0, -145.0, 90.0, -144.0), Rect::full()),
    ];
    for (r, want) in tests {
        let got = r.polar_closure();
        assert!(
            near(got, want),
            "{r:?}.polar_closure() = {got:?}, want {want:?}"
        );
    }
}

// Ported from golang/geo s2/rect_test.go, TestRectCapBound.
#[test]
fn rect_cap_bound() {
    let tests = [
        // The bounding cap at the center is smaller.
        (
            deg(-45.0, -45.0, 45.0, 45.0),
            Cap::from_center_chord_angle(
                Point::from_vector(R3Vector::new(1.0, 0.0, 0.0)),
                ultragraphdb_core_types::ChordAngle::from_squared_length(1.0),
            ),
        ),
        // The bounding cap at the north pole is smaller.
        (
            deg(88.0, -80.0, 89.0, 80.0),
            Cap::from_center_angle(
                Point::from_vector(R3Vector::new(0.0, 0.0, 1.0)),
                Angle::from_degrees(2.0),
            ),
        ),
        // Longitude span > 180 degrees.
        (
            deg(-30.0, -150.0, -10.0, 50.0),
            Cap::from_center_angle(
                Point::from_vector(R3Vector::new(0.0, 0.0, -1.0)),
                Angle::from_degrees(80.0),
            ),
        ),
        // Longitude span > 180 degrees and latitude span > 90 degrees: the polar cap, though larger
        // than the midpoint cap, because the midpoint cap does not contain the whole rect.
        (
            deg(-60.0, -150.0, 70.0, 50.0),
            Cap::from_center_angle(
                Point::from_vector(R3Vector::new(0.0, 0.0, 1.0)),
                Angle::from_degrees(150.0),
            ),
        ),
    ];
    for (r, want) in tests {
        let got = r.cap_bound();
        let ok = want.center().approx_equal(got.center())
            && (want.chord_radius().length2() - got.chord_radius().length2()).abs() <= 1e-14;
        assert!(ok, "{r:?}.cap_bound() = {got:?}, want {want:?}");
    }
    assert!(Rect::empty().cap_bound().is_empty());
}

// Ported from golang/geo s2/rect_test.go, TestRectIntervalOps.
#[test]
fn rect_interval_ops() {
    let r = deg(0.0, -180.0, 90.0, 0.0);
    let mid = deg(45.0, -90.0, 45.0, -90.0);
    let r180 = deg(0.0, -180.0, 0.0, -180.0);
    let north_pole = deg(90.0, 0.0, 90.0, 0.0);
    let tests = [
        (r, mid, true, true, r, mid),
        (r, r180, true, true, r, r180),
        (r, north_pole, true, true, r, north_pole),
        (
            r,
            deg(-10.0, -1.0, 1.0, 20.0),
            false,
            true,
            deg(-10.0, 180.0, 90.0, 20.0),
            deg(0.0, -1.0, 1.0, 0.0),
        ),
        (
            r,
            deg(-10.0, -1.0, 0.0, 20.0),
            false,
            true,
            deg(-10.0, 180.0, 90.0, 20.0),
            deg(0.0, -1.0, 0.0, 0.0),
        ),
        (
            r,
            deg(-10.0, 0.0, 1.0, 20.0),
            false,
            true,
            deg(-10.0, 180.0, 90.0, 20.0),
            deg(0.0, 0.0, 1.0, 0.0),
        ),
        (
            deg(-15.0, -160.0, -15.0, -150.0),
            deg(20.0, 145.0, 25.0, 155.0),
            false,
            false,
            deg(-15.0, 145.0, 25.0, -150.0),
            Rect::empty(),
        ),
        (
            deg(70.0, -10.0, 90.0, -140.0),
            deg(60.0, 175.0, 80.0, 5.0),
            false,
            true,
            deg(60.0, -180.0, 90.0, 180.0),
            deg(70.0, 175.0, 80.0, 5.0),
        ),
        // Overlap in latitude but not longitude, and vice versa.
        (
            deg(12.0, 30.0, 60.0, 60.0),
            deg(0.0, 0.0, 30.0, 18.0),
            false,
            false,
            deg(0.0, 0.0, 60.0, 60.0),
            Rect::empty(),
        ),
        (
            deg(0.0, 0.0, 18.0, 42.0),
            deg(30.0, 12.0, 42.0, 60.0),
            false,
            false,
            deg(0.0, 0.0, 42.0, 60.0),
            Rect::empty(),
        ),
    ];
    for (a, b, contains, intersects, union, intersection) in tests {
        assert_eq!(a.contains(b), contains, "{a:?} contains {b:?}");
        assert_eq!(a.intersects(b), intersects, "{a:?} intersects {b:?}");
        assert_eq!(a.union(b) == a, contains, "{a:?} ∪ {b:?} == {a:?}");
        assert_eq!(
            a.intersection(b).is_empty(),
            !intersects,
            "{a:?} ∩ {b:?} empty"
        );
        assert_eq!(a.union(b), union, "{a:?} ∪ {b:?}");
        assert_eq!(a.intersection(b), intersection, "{a:?} ∩ {b:?}");
    }
}

// Ported from golang/geo s2/rect_test.go, TestRectContainsPoint.
#[test]
fn rect_contains_point() {
    let r = deg(0.0, -180.0, 90.0, 0.0);
    assert!(r.contains_point(Point::from_vector(R3Vector::new(0.5, -0.3, 0.1))));
    assert!(!r.contains_point(Point::from_vector(R3Vector::new(0.5, 0.2, 0.1))));
}

#[test]
fn rect_center_lo_hi() {
    // The bits are Go's Center, Lo and Hi of the same rect.
    let bits = |ll: LatLng| (ll.lat().radians().to_bits(), ll.lng().radians().to_bits());
    let r = deg(-10.0, 170.0, 30.0, -170.0);
    // The longitude interval is inverted: its center is on the antimeridian, at +π.
    assert_eq!(
        bits(r.center()),
        (0x3fc657184ae74486, 0x400921fb54442d18),
        "{:?}",
        r.center()
    );
    assert_eq!(r.center().lng().radians(), PI);
    assert_eq!(bits(r.lo()), (0xbfc657184ae74487, 0x4007bc89cf95b8d0));
    assert_eq!(bits(r.hi()), (0x3fe0c152382d7365, 0xc007bc89cf95b8d0));
}
