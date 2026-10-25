/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported in part from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, r1/interval_test.go.
 * Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! R1Interval predicates, set operations, AddPoint, ClampPoint and Expanded.

use ultragraphdb_core_types::R1Interval;

const UNIT: R1Interval = R1Interval::new(0.0, 1.0);
const NEGUNIT: R1Interval = R1Interval::new(-1.0, 0.0);
const HALF: R1Interval = R1Interval::new(0.5, 0.5);
const EMPTY: R1Interval = R1Interval::empty();

// Ported from golang/geo r1/interval_test.go, TestIsEmpty.
#[test]
fn r1_interval_is_empty() {
    assert!(!UNIT.is_empty());
    assert!(!HALF.is_empty());
    assert!(EMPTY.is_empty());
    assert!(!R1Interval::new(0.0, 0.0).is_empty());
    assert_eq!((EMPTY.lo(), EMPTY.hi()), (1.0, 0.0));
    assert_eq!(R1Interval::from_point(2.5), R1Interval::new(2.5, 2.5));
}

#[test]
fn r1_interval_equal_treats_every_empty_interval_alike() {
    assert!(EMPTY.equal(R1Interval::new(5.0, 3.0)));
    assert!(R1Interval::new(0.0, -2.0).equal(EMPTY));
    assert!(UNIT.equal(R1Interval::new(0.0, 1.0)));
    assert!(!UNIT.equal(HALF));
    assert!(!UNIT.equal(EMPTY));
    assert!(!EMPTY.equal(UNIT));
}

// Ported from golang/geo r1/interval_test.go, TestCenter and TestLength.
#[test]
fn r1_interval_center_and_length() {
    for (i, center, length) in [(UNIT, 0.5, 1.0), (NEGUNIT, -0.5, 1.0), (HALF, 0.5, 0.0)] {
        assert_eq!(i.center(), center, "{i:?}");
        assert_eq!(i.length(), length, "{i:?}");
    }
    assert!(EMPTY.length() < 0.0);
}

// Ported from golang/geo r1/interval_test.go, TestIntervalContains.
#[test]
fn r1_interval_contains() {
    let tests = [
        (UNIT, 0.5, true, true),
        (UNIT, 0.0, true, false),
        (UNIT, 1.0, true, false),
        (UNIT, -1e-300, false, false),
        (UNIT, 1.0 + f64::EPSILON, false, false),
        (EMPTY, 0.5, false, false),
        (UNIT, f64::NAN, false, false),
    ];
    for (i, p, contains, interior) in tests {
        assert_eq!(i.contains(p), contains, "{i:?}.contains({p})");
        assert_eq!(
            i.interior_contains(p),
            interior,
            "{i:?}.interior_contains({p})"
        );
    }
}

// Ported from golang/geo r1/interval_test.go, TestIntervalOperations (ContainsInterval and
// Intersects).
#[test]
fn r1_interval_operations() {
    let tests = [
        (EMPTY, EMPTY, true, false),
        (EMPTY, UNIT, false, false),
        (UNIT, HALF, true, true),
        (UNIT, UNIT, true, true),
        (UNIT, EMPTY, true, false),
        (UNIT, NEGUNIT, false, true),
        (UNIT, R1Interval::new(0.0, 0.5), true, true),
        (HALF, R1Interval::new(0.0, 0.5), false, true),
    ];
    for (have, other, contains, intersects) in tests {
        assert_eq!(
            have.contains_interval(other),
            contains,
            "{have:?} contains {other:?}"
        );
        assert_eq!(
            have.intersects(other),
            intersects,
            "{have:?} intersects {other:?}"
        );
    }
}

// Ported from golang/geo r1/interval_test.go, TestIntersection.
#[test]
fn r1_interval_intersection() {
    let tests = [
        (UNIT, HALF, HALF),
        (UNIT, NEGUNIT, R1Interval::new(0.0, 0.0)),
        (NEGUNIT, HALF, EMPTY),
        (UNIT, EMPTY, EMPTY),
        (EMPTY, UNIT, EMPTY),
    ];
    for (x, y, want) in tests {
        let got = x.intersection(y);
        assert!(got.equal(want), "{x:?} ∩ {y:?} = {got:?}, want {want:?}");
    }
}

// Ported from golang/geo r1/interval_test.go, TestUnion.
#[test]
fn r1_interval_union() {
    let tests = [
        (
            R1Interval::new(99.0, 100.0),
            EMPTY,
            R1Interval::new(99.0, 100.0),
        ),
        (
            EMPTY,
            R1Interval::new(99.0, 100.0),
            R1Interval::new(99.0, 100.0),
        ),
        (R1Interval::new(5.0, 3.0), R1Interval::new(0.0, -2.0), EMPTY),
        (R1Interval::new(0.0, -2.0), R1Interval::new(5.0, 3.0), EMPTY),
        (UNIT, UNIT, UNIT),
        (UNIT, NEGUNIT, R1Interval::new(-1.0, 1.0)),
        (NEGUNIT, UNIT, R1Interval::new(-1.0, 1.0)),
        (HALF, UNIT, UNIT),
    ];
    for (x, y, want) in tests {
        let got = x.union(y);
        assert!(got.equal(want), "{x:?} ∪ {y:?} = {got:?}, want {want:?}");
    }
}

// Ported from golang/geo r1/interval_test.go, TestAddPoint.
#[test]
fn r1_interval_add_point() {
    let tests = [
        (EMPTY, 5.0, R1Interval::new(5.0, 5.0)),
        (R1Interval::new(5.0, 5.0), -1.0, R1Interval::new(-1.0, 5.0)),
        (R1Interval::new(-1.0, 5.0), 0.0, R1Interval::new(-1.0, 5.0)),
        (R1Interval::new(-1.0, 5.0), 6.0, R1Interval::new(-1.0, 6.0)),
    ];
    for (i, p, want) in tests {
        assert_eq!(i.add_point(p), want, "{i:?}.add_point({p})");
    }
}

// Ported from golang/geo r1/interval_test.go, TestClampPoint.
#[test]
fn r1_interval_clamp_point() {
    let i = R1Interval::new(0.1, 0.4);
    for (p, want) in [(0.3, 0.3), (-7.0, 0.1), (0.6, 0.4), (0.1, 0.1), (0.4, 0.4)] {
        assert_eq!(i.clamp_point(p), want, "clamp_point({p})");
    }
}

// Ported from golang/geo r1/interval_test.go, TestExpanded.
#[test]
fn r1_interval_expanded() {
    let tests = [
        (EMPTY, 0.45, EMPTY),
        (UNIT, 0.5, R1Interval::new(-0.5, 1.5)),
        (UNIT, -0.5, R1Interval::new(0.5, 0.5)),
        (UNIT, -0.51, EMPTY),
    ];
    for (i, margin, want) in tests {
        let got = i.expanded(margin);
        assert!(
            got.equal(want),
            "{i:?}.expanded({margin}) = {got:?}, want {want:?}"
        );
    }
    // An empty interval stays exactly as it is.
    assert_eq!(
        R1Interval::new(3.0, 1.0).expanded(5.0),
        R1Interval::new(3.0, 1.0)
    );
}
