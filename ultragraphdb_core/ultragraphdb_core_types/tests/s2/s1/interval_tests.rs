/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported in part from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s1/interval_test.go.
 * Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! S1Interval predicates, center, length, set operations, AddPoint and Expanded.

use core::f64::consts::PI;
use ultragraphdb_core_types::S1Interval;

const MACHINE_EPSILON: f64 = f64::EPSILON;

fn ep(lo: f64, hi: f64) -> S1Interval {
    S1Interval::from_endpoints(lo, hi)
}

/// The standard intervals of Go's test: the quadrants are quad1 == [0, π/2], quad2 == [π/2, π],
/// quad3 == [-π, -π/2] and quad4 == [-π/2, 0].
struct Fixtures {
    empty: S1Interval,
    full: S1Interval,
    zero: S1Interval,
    pi2: S1Interval,
    pi: S1Interval,
    mipi: S1Interval,
    mipi2: S1Interval,
    quad1: S1Interval,
    quad2: S1Interval,
    quad3: S1Interval,
    quad4: S1Interval,
    quad12: S1Interval,
    quad23: S1Interval,
    quad34: S1Interval,
    quad123: S1Interval,
    quad234: S1Interval,
    quad341: S1Interval,
    quad412: S1Interval,
    mid12: S1Interval,
    mid23: S1Interval,
    mid34: S1Interval,
    mid41: S1Interval,
}

fn fixtures() -> Fixtures {
    Fixtures {
        empty: S1Interval::empty(),
        full: S1Interval::full(),
        zero: ep(0.0, 0.0),
        pi2: ep(PI / 2.0, PI / 2.0),
        pi: ep(PI, PI),
        mipi: ep(-PI, -PI),
        mipi2: ep(-PI / 2.0, -PI / 2.0),
        quad1: ep(0.0, PI / 2.0),
        quad2: ep(PI / 2.0, -PI),
        quad3: ep(PI, -PI / 2.0),
        quad4: ep(-PI / 2.0, 0.0),
        quad12: ep(0.0, -PI),
        quad23: ep(PI / 2.0, -PI / 2.0),
        quad34: ep(-PI, 0.0),
        quad123: ep(0.0, -PI / 2.0),
        quad234: ep(PI / 2.0, 0.0),
        quad341: ep(PI, PI / 2.0),
        quad412: ep(-PI / 2.0, -PI),
        mid12: ep(PI / 2.0 - 0.01, PI / 2.0 + 0.02),
        mid23: ep(PI - 0.01, -PI + 0.02),
        mid34: ep(-PI / 2.0 - 0.01, -PI / 2.0 + 0.02),
        mid41: ep(-0.01, 0.02),
    }
}

fn near(a: S1Interval, b: S1Interval) -> bool {
    (a.lo() - b.lo()).abs() <= 1e-15 && (a.hi() - b.hi()).abs() <= 1e-15
}

// Ported from golang/geo s1/interval_test.go, TestConstructors.
#[test]
fn s1_interval_constructors() {
    let t = fixtures();
    // [-π,-π] is normalized to [π,π].
    assert_eq!((t.mipi.lo(), t.mipi.hi()), (PI, PI));
    // [-π, x] and [x, -π] normalize -π to π, except for the full interval.
    assert_eq!((ep(-PI, 1.0).lo(), ep(-PI, 1.0).hi()), (PI, 1.0));
    assert_eq!((ep(1.0, -PI).lo(), ep(1.0, -PI).hi()), (1.0, PI));
    assert_eq!(ep(-PI, PI), S1Interval::full());
    assert_eq!(ep(PI, -PI), S1Interval::empty());
    // new keeps its endpoints as given.
    assert_eq!(
        (
            S1Interval::new(-PI, 1.0).lo(),
            S1Interval::new(-PI, 1.0).hi()
        ),
        (-PI, 1.0)
    );
    assert!(S1Interval::new(0.0, 0.0).is_valid());
}

// Ported from golang/geo s1/interval_test.go, TestSimplePredicates.
#[test]
fn s1_interval_simple_predicates() {
    let t = fixtures();
    assert!(t.zero.is_valid() && !t.zero.is_empty() && !t.zero.is_full());
    assert!(t.empty.is_valid() && t.empty.is_empty() && !t.empty.is_full());
    assert!(t.empty.is_inverted());
    assert!(t.full.is_valid() && !t.full.is_empty() && t.full.is_full());
    assert!(t.pi.is_valid() && !t.pi.is_empty() && !t.pi.is_inverted());
    assert!(t.mipi.is_valid() && !t.mipi.is_empty() && !t.mipi.is_inverted());
    // Invalid intervals: an endpoint past π, and -π paired with anything but π.
    for (lo, hi) in [
        (0.0, PI + 1e-15),
        (-PI - 1e-15, 0.0),
        (-PI, 0.0),
        (0.0, -PI),
        (f64::NAN, 0.0),
    ] {
        assert!(!S1Interval::new(lo, hi).is_valid(), "[{lo}, {hi}]");
    }
}

// Ported from golang/geo s1/interval_test.go, TestAlmostFullOrEmpty.
#[test]
fn s1_interval_almost_full_or_empty() {
    // The greatest representable value less than π.
    let almost_pi = PI - 2.0 * MACHINE_EPSILON;
    assert!(!S1Interval::new(-almost_pi, PI).is_full());
    assert!(!S1Interval::new(-PI, almost_pi).is_full());
    assert!(!S1Interval::new(PI, -almost_pi).is_empty());
    assert!(!S1Interval::new(almost_pi, -PI).is_empty());
}

// Ported from golang/geo s1/interval_test.go, TestCenter.
#[test]
fn s1_interval_center() {
    let t = fixtures();
    let tests = [
        (t.quad12, PI / 2.0),
        (ep(3.1, 2.9), 3.0 - PI),
        (ep(-2.9, -3.1), PI - 3.0),
        (ep(2.1, -2.1), PI),
        (t.pi, PI),
        (t.mipi, PI),
        (t.quad23, PI),
        (t.quad123, 0.75 * PI),
    ];
    for (i, want) in tests {
        let got = i.center();
        assert!(
            (got - want).abs() <= 1e-15,
            "{i:?}.center() = {got}, want {want}"
        );
    }
}

// Ported from golang/geo s1/interval_test.go, TestLength.
#[test]
fn s1_interval_length() {
    let t = fixtures();
    let tests = [
        (t.quad12, PI),
        (t.pi, 0.0),
        (t.mipi, 0.0),
        (t.quad123, 1.5 * PI),
        (t.quad23, PI),
        (t.full, 2.0 * PI),
    ];
    for (i, want) in tests {
        assert_eq!(i.length(), want, "{i:?}");
    }
    assert!(t.empty.length() < 0.0);
    assert_eq!(t.empty.length(), -1.0);
}

// Ported from golang/geo s1/interval_test.go, TestContains (the Contains part).
#[test]
fn s1_interval_contains() {
    let t = fixtures();
    let tests: [(S1Interval, &[f64], &[f64]); 7] = [
        (t.empty, &[], &[0.0, PI, -PI]),
        (t.full, &[0.0, PI, -PI], &[]),
        (t.quad12, &[0.0, PI, -PI], &[]),
        (t.quad23, &[PI / 2.0, -PI / 2.0, PI, -PI], &[0.0]),
        (t.pi, &[PI, -PI], &[0.0]),
        (t.mipi, &[PI, -PI], &[0.0]),
        (t.zero, &[0.0], &[]),
    ];
    for (i, inside, outside) in tests {
        for &p in inside {
            assert!(i.contains(p), "{i:?} should contain {p}");
        }
        for &p in outside {
            assert!(!i.contains(p), "{i:?} should not contain {p}");
        }
    }
}

// Ported from golang/geo s1/interval_test.go, TestIntervalOperations (ContainsInterval, Intersects,
// Union and Intersection; the interior predicates are not ported).
#[test]
fn s1_interval_operations() {
    let t = fixtures();
    let quad12eps = ep(t.quad12.lo(), t.mid23.hi());
    let quad2hi = ep(t.mid23.lo(), t.quad12.hi());
    let quad412eps = ep(t.mid34.lo(), t.quad12.hi());
    let quadeps12 = ep(t.mid41.lo(), t.quad12.hi());
    let quad1lo = ep(t.quad12.lo(), t.mid41.hi());
    let quad2lo = ep(t.quad23.lo(), t.mid12.hi());
    let quad3hi = ep(t.mid34.lo(), t.quad23.hi());
    let quadeps23 = ep(t.mid12.lo(), t.quad23.hi());
    let quad23eps = ep(t.quad23.lo(), t.mid34.hi());
    let quadeps123 = ep(t.mid41.lo(), t.quad23.hi());
    let (empty, full, zero, pi2, pi, mipi, mipi2) =
        (t.empty, t.full, t.zero, t.pi2, t.pi, t.mipi, t.mipi2);
    let (quad1, quad2, quad3, quad4) = (t.quad1, t.quad2, t.quad3, t.quad4);
    let (quad12, quad23, quad34, quad123, quad234) =
        (t.quad12, t.quad23, t.quad34, t.quad123, t.quad234);
    let (mid12, mid23, mid34, mid41) = (t.mid12, t.mid23, t.mid34, t.mid41);

    // (x, y, x contains y, x intersects y, union, intersection)
    let tests = [
        (empty, empty, true, false, empty, empty),
        (empty, full, false, false, full, empty),
        (empty, zero, false, false, zero, empty),
        (empty, pi, false, false, pi, empty),
        (empty, mipi, false, false, mipi, empty),
        (full, empty, true, false, full, empty),
        (full, full, true, true, full, full),
        (full, zero, true, true, full, zero),
        (full, pi, true, true, full, pi),
        (full, mipi, true, true, full, mipi),
        (full, quad12, true, true, full, quad12),
        (full, quad23, true, true, full, quad23),
        (zero, empty, true, false, zero, empty),
        (zero, full, false, true, full, zero),
        (zero, zero, true, true, zero, zero),
        (zero, pi, false, false, ep(0.0, PI), empty),
        (zero, pi2, false, false, quad1, empty),
        (zero, mipi, false, false, quad12, empty),
        (zero, mipi2, false, false, quad4, empty),
        (zero, quad12, false, true, quad12, zero),
        (zero, quad23, false, false, quad123, empty),
        (pi2, empty, true, false, pi2, empty),
        (pi2, full, false, true, full, pi2),
        (pi2, zero, false, false, quad1, empty),
        (pi2, pi, false, false, ep(PI / 2.0, PI), empty),
        (pi2, pi2, true, true, pi2, pi2),
        (pi2, mipi, false, false, quad2, empty),
        (pi2, mipi2, false, false, quad23, empty),
        (pi2, quad12, false, true, quad12, pi2),
        (pi2, quad23, false, true, quad23, pi2),
        (pi, empty, true, false, pi, empty),
        (pi, full, false, true, full, pi),
        (pi, zero, false, false, ep(PI, 0.0), empty),
        (pi, pi, true, true, pi, pi),
        (pi, pi2, false, false, ep(PI / 2.0, PI), empty),
        (pi, mipi, true, true, pi, pi),
        (pi, mipi2, false, false, quad3, empty),
        (pi, quad12, false, true, ep(0.0, PI), pi),
        (pi, quad23, false, true, quad23, pi),
        (mipi, empty, true, false, mipi, empty),
        (mipi, full, false, true, full, mipi),
        (mipi, zero, false, false, quad34, empty),
        (mipi, pi, true, true, mipi, mipi),
        (mipi, pi2, false, false, quad2, empty),
        (mipi, mipi, true, true, mipi, mipi),
        (mipi, mipi2, false, false, ep(-PI, -PI / 2.0), empty),
        (mipi, quad12, false, true, quad12, mipi),
        (mipi, quad23, false, true, quad23, mipi),
        (quad12, empty, true, false, quad12, empty),
        (quad12, full, false, true, full, quad12),
        (quad12, zero, true, true, quad12, zero),
        (quad12, pi, true, true, quad12, pi),
        (quad12, mipi, true, true, quad12, mipi),
        (quad12, quad12, true, true, quad12, quad12),
        (quad12, quad23, false, true, quad123, quad2),
        (quad12, quad34, false, true, full, quad12),
        (quad23, empty, true, false, quad23, empty),
        (quad23, full, false, true, full, quad23),
        (quad23, zero, false, false, quad234, empty),
        (quad23, pi, true, true, quad23, pi),
        (quad23, mipi, true, true, quad23, mipi),
        (quad23, quad12, false, true, quad123, quad2),
        (quad23, quad23, true, true, quad23, quad23),
        (quad23, quad34, false, true, quad234, ep(-PI, -PI / 2.0)),
        (quad1, quad23, false, true, quad123, ep(PI / 2.0, PI / 2.0)),
        (quad2, quad3, false, true, quad23, mipi),
        (quad3, quad2, false, true, quad23, pi),
        (quad2, pi, true, true, quad2, pi),
        (quad2, mipi, true, true, quad2, mipi),
        (quad3, pi, true, true, quad3, pi),
        (quad3, mipi, true, true, quad3, mipi),
        (quad12, mid12, true, true, quad12, mid12),
        (mid12, quad12, false, true, quad12, mid12),
        (quad12, mid23, false, true, quad12eps, quad2hi),
        (mid23, quad12, false, true, quad12eps, quad2hi),
        // The union of two disjoint intervals is the smallest interval containing both; the center
        // of mid34 is slightly CCW of -π/2, so the result is unambiguous.
        (quad12, mid34, false, false, quad412eps, empty),
        (mid34, quad12, false, false, quad412eps, empty),
        (quad12, mid41, false, true, quadeps12, quad1lo),
        (mid41, quad12, false, true, quadeps12, quad1lo),
        (quad23, mid12, false, true, quadeps23, quad2lo),
        (mid12, quad23, false, true, quadeps23, quad2lo),
        (quad23, mid23, true, true, quad23, mid23),
        (mid23, quad23, false, true, quad23, mid23),
        (quad23, mid34, false, true, quad23eps, quad3hi),
        (mid34, quad23, false, true, quad23eps, quad3hi),
        (quad23, mid41, false, false, quadeps123, empty),
        (mid41, quad23, false, false, quadeps123, empty),
    ];
    for (n, (x, y, contains, intersects, union, intersection)) in tests.into_iter().enumerate() {
        assert_eq!(
            x.contains_interval(y),
            contains,
            "{n}: {x:?} contains {y:?}"
        );
        assert_eq!(x.intersects(y), intersects, "{n}: {x:?} intersects {y:?}");
        assert_eq!(x.union(y), union, "{n}: {x:?} ∪ {y:?}");
        assert_eq!(x.intersection(y), intersection, "{n}: {x:?} ∩ {y:?}");
    }
}

// Ported from golang/geo s1/interval_test.go, TestAddPoint.
#[test]
fn s1_interval_add_point() {
    let t = fixtures();
    let tests: [(S1Interval, &[f64], S1Interval); 12] = [
        (t.empty, &[0.0], t.zero),
        (t.empty, &[PI], t.pi),
        (t.empty, &[-PI], t.mipi),
        (t.empty, &[PI, -PI], t.pi),
        (t.empty, &[-PI, PI], t.mipi),
        (t.empty, &[t.mid12.lo(), t.mid12.hi()], t.mid12),
        (t.empty, &[t.mid23.lo(), t.mid23.hi()], t.mid23),
        (t.quad1, &[-0.9 * PI, -PI / 2.0], t.quad123),
        (t.full, &[0.0], t.full),
        (t.full, &[PI], t.full),
        (t.full, &[-PI], t.full),
        // A point outside [-π, π] is ignored.
        (t.quad1, &[PI + 0.5, -4.0, f64::INFINITY], t.quad1),
    ];
    for (i, points, want) in tests {
        let got = points.iter().fold(i, |acc, &p| acc.add_point(p));
        assert!(
            near(got, want),
            "{i:?}.add_point({points:?}) = {got:?}, want {want:?}"
        );
    }
}

// Ported from golang/geo s1/interval_test.go, TestExpanded.
#[test]
fn s1_interval_expanded() {
    let t = fixtures();
    let tests = [
        (t.empty, 1.0, t.empty),
        (t.full, 1.0, t.full),
        (t.zero, 1.0, S1Interval::new(-1.0, 1.0)),
        (t.mipi, 0.01, S1Interval::new(PI - 0.01, -PI + 0.01)),
        (t.pi, 27.0, t.full),
        (t.pi, PI / 2.0, t.quad23),
        (t.pi2, PI / 2.0, t.quad12),
        (t.mipi2, PI / 2.0, t.quad34),
        (t.empty, -1.0, t.empty),
        (t.full, -1.0, t.full),
        (t.quad123, -27.0, t.empty),
        (t.quad234, -27.0, t.empty),
        (t.quad123, -PI / 2.0, t.quad2),
        (t.quad341, -PI / 2.0, t.quad4),
        (t.quad412, -PI / 2.0, t.quad1),
    ];
    for (i, margin, want) in tests {
        let got = i.expanded(margin);
        assert!(
            near(got, want),
            "{i:?}.expanded({margin}) = {got:?}, want {want:?}"
        );
    }
}
