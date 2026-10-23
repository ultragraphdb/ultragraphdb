/*
 * Copyright 2016 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported in part from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/predicates_test.go; the golden-vector checks are new.
 * Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Sign, RobustSign (with its exact and symbolic fallbacks) and AngleContainsVertex.

use super::golden::{Checker, load, pt};
use serde::Deserialize;
use ultragraphdb_core_types::{
    Angle, Direction, Loop, Point, R3Vector, angle_contains_vertex, robust_sign, sign,
};

#[derive(Deserialize)]
pub struct SignCase {
    pub a: [String; 3],
    pub b: [String; 3],
    pub c: [String; 3],
    pub sign: bool,
    pub robust: i32,
    pub angle_contains_vertex: bool,
}

#[derive(Deserialize)]
pub struct CrossCase {
    pub a: [String; 3],
    pub b: [String; 3],
    pub c: [String; 3],
    pub d: [String; 3],
    pub crossing: i32,
    pub vertex: bool,
    pub edge_or_vertex: bool,
    pub ordered_ccw: bool,
}

#[derive(Deserialize)]
pub struct ChainCase {
    pub a: [String; 3],
    pub b: [String; 3],
    pub chain: Vec<[String; 3]>,
    pub chain_crossing_sign: Vec<i32>,
    pub edge_or_vertex_chain: Vec<bool>,
    pub mixed: Vec<i32>,
}

#[derive(Deserialize)]
pub struct Golden {
    pub signs: Vec<SignCase>,
    pub crossings: Vec<CrossCase>,
    pub chains: Vec<ChainCase>,
}

#[test]
fn signs_match_go() {
    let g: Golden = load("predicates.json");
    let mut c = Checker::new("predicates");
    for k in &g.signs {
        let (a, b, p) = (pt(&k.a), pt(&k.b), pt(&k.c));
        c.check(sign(a, b, p) == k.sign, || {
            format!("sign({a:?}, {b:?}, {p:?})")
        });
        let r = robust_sign(a, b, p).value();
        c.check(r == k.robust, || {
            format!("robust_sign({a:?}, {b:?}, {p:?}) = {r} want {}", k.robust)
        });
        let acv = angle_contains_vertex(a, b, p);
        c.check(acv == k.angle_contains_vertex, || {
            format!("angle_contains_vertex({a:?}, {b:?}, {p:?}) = {acv}")
        });
    }
    c.finish();
}

fn p(x: f64, y: f64, z: f64) -> Point {
    Point::from_vector(R3Vector::new(x, y, z))
}

// Ported from golang/geo s2/predicates_test.go, TestPredicatesSign.
#[test]
fn predicates_sign() {
    let tests: [([f64; 9], bool); 13] = [
        ([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], true),
        ([0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0], true),
        ([0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], true),
        ([1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 1.0], true),
        ([-3.0, -1.0, 4.0, 2.0, -1.0, -3.0, 1.0, -2.0, 0.0], true),
        // All degenerate cases of Sign(). Let M_1, M_2, ... be the sequence of
        // submatrices whose determinant sign is tested by that function.
        ([-3.0, -1.0, 0.0, -2.0, 1.0, 0.0, 1.0, -2.0, 0.0], false), // det(M_1) = b0*c1 - b1*c0
        ([-6.0, 3.0, 3.0, -4.0, 2.0, -1.0, -2.0, 1.0, 4.0], false), // det(M_2) = b2*c0 - b0*c2
        ([0.0, -1.0, -1.0, 0.0, 1.0, -2.0, 0.0, 2.0, 1.0], false),  // det(M_3) = b1*c2 - b2*c1
        ([-1.0, 2.0, 7.0, 2.0, 1.0, -4.0, 4.0, 2.0, -8.0], false),  // det(M_4) = c0*a1 - c1*a0
        ([-4.0, -2.0, 7.0, 2.0, 1.0, -4.0, 4.0, 2.0, -8.0], false), // det(M_5) = c0
        ([0.0, -5.0, 7.0, 0.0, -4.0, 8.0, 0.0, -2.0, 4.0], false),  // det(M_6) = -c1
        ([-5.0, -2.0, 7.0, 0.0, 0.0, -2.0, 0.0, 0.0, -1.0], false), // det(M_7) = c2*a0 - c0*a2
        ([0.0, -2.0, 7.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0], false),    // det(M_8) = c2
    ];
    for (v, want) in tests {
        let (p1, p2, p3) = (
            p(v[0], v[1], v[2]),
            p(v[3], v[4], v[5]),
            p(v[6], v[7], v[8]),
        );
        assert_eq!(sign(p1, p2, p3), want, "{v:?}");
        if want {
            // For these cases we can test the reversibility condition
            assert_ne!(sign(p3, p2, p1), want, "reversed {v:?}");
        }
    }
}

// Ported from golang/geo s2/predicates_test.go, TestPredicatesRobustSign. The literals are
// Go's, verbatim.
#[test]
#[allow(clippy::excessive_precision)]
fn predicates_robust_sign() {
    let x = p(1.0, 0.0, 0.0);
    let y = p(0.0, 1.0, 0.0);
    let z = p(0.0, 0.0, 1.0);

    // The following points happen to be *exactly collinear* along a line that it
    // approximate tangent to the surface of the unit sphere. In fact, C is the
    // exact midpoint of the line segment AB.
    let po_a = p(
        0.72571927877036835,
        0.46058825605889098,
        0.51106749730504852,
    );
    let po_b = p(0.7257192746638208, 0.46058826573818168, 0.51106749441312738);
    let po_c = p(
        0.72571927671709457,
        0.46058826089853633,
        0.51106749585908795,
    );

    // The points "x1" and "x2" are exactly proportional.
    let x1 = p(0.99999999999999989, 1.4901161193847655e-08, 0.0);
    let x2 = p(1.0, 1.4901161193847656e-08, 0.0);

    // Two more points that are distinct, exactly proportional.
    let x3 = Point::from_vector(R3Vector::new(1.0, 1.0, 1.0).normalize());
    let x4 = Point::from_vector(x3.vector().mul(0.99999999999999989));

    // Normalize() is not idempotent.
    let y0 = R3Vector::new(1.0, 1.0, 0.0);
    let y1 = Point::from_vector(y0.normalize());
    let y2 = Point::from_vector(y1.vector().normalize());

    let neg = |q: Point| Point::from_vector(q.vector().mul(-1.0));
    let tests = [
        (x, x, z, Direction::Indeterminate),
        (x, y, y, Direction::Indeterminate),
        (z, x, z, Direction::Indeterminate),
        (x, y, z, Direction::CounterClockwise),
        (z, y, x, Direction::Clockwise),
        (po_a, po_b, po_c, Direction::Clockwise),
        (x1, x2, neg(x1), Direction::CounterClockwise),
        (x3, x4, neg(x3), Direction::Clockwise),
        (y1, y2, neg(y1), Direction::CounterClockwise),
    ];
    for (p1, p2, p3, want) in tests {
        let result = robust_sign(p1, p2, p3);
        assert_eq!(result, want, "robust_sign({p1:?}, {p2:?}, {p3:?})");
        // Test RobustSign(b,c,a) == RobustSign(a,b,c) for all a,b,c
        assert_eq!(robust_sign(p2, p3, p1), result);
        // Test RobustSign(c,b,a) == -RobustSign(a,b,c) for all a,b,c
        assert_eq!(robust_sign(p3, p2, p1), -result);
    }
}

// Ported from golang/geo s2/edge_crossings_test.go, TestAngleContainsVertex.
#[test]
fn angle_contains_vertex_cases() {
    let a = Point::from_coords(1.0, 0.0, 0.0);
    let b = Point::from_coords(0.0, 1.0, 0.0);
    // Go's referenceDir(B) is the unexported Ortho(B): B x (1, 0.0053, 0.00457), normalized,
    // since the largest component of B is y.
    let ref_b = Point::from_vector(
        b.vector()
            .cross(R3Vector::new(1.0, 0.0053, 0.00457))
            .normalize(),
    );

    // Degenerate angle ABA.
    assert!(!angle_contains_vertex(a, b, a));
    // An angle where A == referenceDir(B).
    assert!(angle_contains_vertex(ref_b, b, a));
    // An angle where C == referenceDir(B).
    assert!(!angle_contains_vertex(a, b, ref_b));

    // Verify that when a set of polygons tile the region around the vertex,
    // exactly one of those polygons contains the vertex.
    let l = Loop::regular(b, Angle::from_degrees(10.0), 10);
    let count = (0..l.num_vertices())
        .filter(|&i| angle_contains_vertex(l.vertex(i + 1), b, l.vertex(i)))
        .count();
    assert_eq!(count, 1);
}

#[test]
fn robust_sign_of_points_that_are_not_finite_is_indeterminate() {
    // Go's big.Float panics on NaN in the exact predicate.
    use ultragraphdb_core_types::{Direction, Point, R3Vector, robust_sign};
    let nan = Point::from_vector(R3Vector::new(f64::NAN, 0.0, 1.0));
    let inf = Point::from_vector(R3Vector::new(f64::INFINITY, 0.0, 1.0));
    let a = Point::from_vector(R3Vector::new(1.0, 0.0, 0.0));
    let b = Point::from_vector(R3Vector::new(0.0, 1.0, 0.0));
    assert_eq!(robust_sign(a, b, nan), Direction::Indeterminate);
    assert_eq!(robust_sign(a, inf, b), Direction::Indeterminate);
}
