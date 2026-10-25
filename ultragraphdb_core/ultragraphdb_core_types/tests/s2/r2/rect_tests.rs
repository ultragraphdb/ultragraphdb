/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported in part from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, r2/rect_test.go.
 * Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! R2Rect construction, vertices, containment, set operations and expansion.

use ultragraphdb_core_types::{R1Interval, R2Point, R2Rect};

fn p(x: f64, y: f64) -> R2Point {
    R2Point::new(x, y)
}

fn from(a: R2Point, b: R2Point) -> R2Rect {
    R2Rect::from_points(&[a, b])
}

fn approx(a: R2Rect, b: R2Rect) -> bool {
    // Go's r1.Interval.ApproxEqual with its epsilon 1e-15, on both axes.
    let iv = |i: R1Interval, o: R1Interval| {
        if i.is_empty() {
            return o.length() <= 2e-15;
        }
        if o.is_empty() {
            return i.length() <= 2e-15;
        }
        (o.lo() - i.lo()).abs() <= 1e-15 && (o.hi() - i.hi()).abs() <= 1e-15
    };
    iv(a.x(), b.x()) && iv(a.y(), b.y())
}

const SW: R2Point = R2Point::new(0.0, 0.25);
const SE: R2Point = R2Point::new(0.5, 0.25);
const NE: R2Point = R2Point::new(0.5, 0.75);
const NW: R2Point = R2Point::new(0.0, 0.75);

fn rect() -> R2Rect {
    from(SW, NE)
}

// Ported from golang/geo r2/rect_test.go, TestEmptyRect.
#[test]
fn r2_rect_empty() {
    let e = R2Rect::empty();
    assert!(e.is_empty());
    assert!(e.x().is_empty() && e.y().is_empty());
    assert!(!R2Rect::default().is_empty());
    assert_eq!(
        R2Rect::default(),
        R2Rect::new(R1Interval::new(0.0, 0.0), R1Interval::new(0.0, 0.0))
    );
    // Go's RectFromPoints of no points is the zero Rect.
    assert_eq!(R2Rect::from_points(&[]), R2Rect::default());
}

// Ported from golang/geo r2/rect_test.go, TestFromVariousTypes (the RectFromPoints cases).
#[test]
fn r2_rect_from_points() {
    let d1 = from(p(0.1, 0.0), p(0.25, 1.0));
    assert_eq!(
        d1,
        R2Rect::new(R1Interval::new(0.1, 0.25), R1Interval::new(0.0, 1.0))
    );
    let tests = [
        (
            from(p(0.15, 0.3), p(0.35, 0.9)),
            from(p(0.15, 0.9), p(0.35, 0.3)),
        ),
        (
            from(p(0.12, 0.0), p(0.83, 0.5)),
            from(p(0.83, 0.0), p(0.12, 0.5)),
        ),
    ];
    for (a, b) in tests {
        assert_eq!(a, b);
    }
    // One point gives a degenerate rect at that point, not one stretched to the origin.
    assert_eq!(
        R2Rect::from_points(&[p(0.2, 0.3)]),
        R2Rect::new(R1Interval::new(0.2, 0.2), R1Interval::new(0.3, 0.3))
    );
}

// Ported from golang/geo r2/rect_test.go, TestCenter and TestVertices.
#[test]
fn r2_rect_center_and_vertices() {
    assert_eq!(R2Rect::empty().center(), p(0.5, 0.5));
    assert_eq!(rect().center(), p(0.25, 0.5));
    assert_eq!(rect().vertices(), [SW, SE, NE, NW]);
    assert_eq!(rect().vertex_ij(0, 0), SW);
    assert_eq!(rect().vertex_ij(1, 0), SE);
    assert_eq!(rect().vertex_ij(1, 1), NE);
    assert_eq!(rect().vertex_ij(0, 1), NW);
}

// Ported from golang/geo r2/rect_test.go, TestContainsPoint.
#[test]
fn r2_rect_contains_point() {
    let r = rect();
    let tests = [
        (p(0.2, 0.4), true),
        (p(0.2, 0.8), false),
        (p(-0.1, 0.4), false),
        (p(0.6, 0.1), false),
        (p(r.x().lo(), r.y().lo()), true),
        (p(r.x().hi(), r.y().hi()), true),
    ];
    for (q, want) in tests {
        assert_eq!(r.contains_point(q), want, "{q:?}");
    }
    assert!(!R2Rect::empty().contains_point(p(0.5, 0.5)));
}

// Ported from golang/geo r2/rect_test.go, TestIntervalOps (Intersects, Union and AddRect).
#[test]
fn r2_rect_interval_ops() {
    let r = rect();
    let mid = from(p(0.25, 0.5), p(0.25, 0.5));
    let rsw = from(SW, SW);
    let rne = from(NE, NE);
    let tests = [
        (r, mid, true, r),
        (r, rsw, true, r),
        (r, rne, true, r),
        (
            r,
            from(p(0.45, 0.1), p(0.75, 0.3)),
            true,
            from(p(0.0, 0.1), p(0.75, 0.75)),
        ),
        (
            r,
            from(p(0.5, 0.1), p(0.7, 0.3)),
            true,
            from(p(0.0, 0.1), p(0.7, 0.75)),
        ),
        (
            r,
            from(p(0.45, 0.1), p(0.7, 0.25)),
            true,
            from(p(0.0, 0.1), p(0.7, 0.75)),
        ),
        (
            from(p(0.1, 0.2), p(0.1, 0.3)),
            from(p(0.15, 0.7), p(0.2, 0.8)),
            false,
            from(p(0.1, 0.2), p(0.2, 0.8)),
        ),
        // Overlap in x but not y, and vice versa.
        (
            from(p(0.1, 0.2), p(0.4, 0.5)),
            from(p(0.0, 0.0), p(0.2, 0.1)),
            false,
            from(p(0.0, 0.0), p(0.4, 0.5)),
        ),
        (
            from(p(0.0, 0.0), p(0.1, 0.3)),
            from(p(0.2, 0.1), p(0.3, 0.4)),
            false,
            from(p(0.0, 0.0), p(0.3, 0.4)),
        ),
    ];
    for (a, b, intersects, union) in tests {
        assert_eq!(a.intersects(b), intersects, "{a:?} intersects {b:?}");
        assert_eq!(a.union(b), union, "{a:?} ∪ {b:?}");
        assert_eq!(a.add_rect(b), union, "{a:?}.add_rect({b:?})");
    }
    assert!(!r.intersects(R2Rect::empty()));
    assert_eq!(r.union(R2Rect::empty()), r);
}

// Ported from golang/geo r2/rect_test.go, TestAddPoint.
#[test]
fn r2_rect_add_point() {
    let got = [SW, SE, NW, p(0.1, 0.4)]
        .into_iter()
        .fold(R2Rect::empty(), |r, q| r.add_point(q));
    assert!(approx(got, rect()), "{got:?}");
}

// Ported from golang/geo r2/rect_test.go, TestExpandedEmpty and TestExpandedEqual.
#[test]
fn r2_rect_expanded() {
    let r = from(p(0.2, 0.4), p(0.3, 0.7));
    for (have, margin) in [
        (R2Rect::empty(), p(0.1, 0.3)),
        (R2Rect::empty(), p(-0.1, -0.3)),
        (r, p(-0.1, 0.3)),
        (r, p(0.1, -0.2)),
    ] {
        let got = have.expanded(margin);
        assert!(got.is_empty(), "{have:?}.expanded({margin:?}) = {got:?}");
        assert_eq!(got, R2Rect::empty());
    }
    let tests = [
        (p(0.1, 0.3), from(p(0.1, 0.1), p(0.4, 1.0))),
        (p(0.1, -0.1), from(p(0.1, 0.5), p(0.4, 0.6))),
        (p(0.1, 0.1), from(p(0.1, 0.3), p(0.4, 0.8))),
    ];
    for (margin, want) in tests {
        let got = r.expanded(margin);
        assert!(
            approx(got, want),
            "{r:?}.expanded({margin:?}) = {got:?}, want {want:?}"
        );
    }
    assert!(approx(
        r.expanded_by_margin(0.1),
        from(p(0.1, 0.3), p(0.4, 0.8))
    ));
    assert!(r.expanded_by_margin(-0.06).is_empty());
}
