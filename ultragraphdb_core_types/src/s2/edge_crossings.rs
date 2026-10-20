/*
 * Copyright 2017 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/edge_crossings.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::s2::edge_crosser::EdgeCrosser;
use crate::s2::point::{Point, ordered_ccw};

/// How edges cross.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Crossing {
    /// The edges cross.
    Cross = 0,
    /// Two vertices from different edges are the same.
    MaybeCross = 1,
    /// The edges do not cross.
    DoNotCross = 2,
}

impl Crossing {
    /// The crossing as Go's int value: Cross 0, MaybeCross 1, DoNotCross 2.
    pub fn value(&self) -> i32 {
        *self as i32
    }
}

/// Reports whether the edge AB intersects the edge CD. If AB crosses CD at a point that is
/// interior to both edges, Cross is returned. If any two vertices from different edges are
/// the same it returns MaybeCross. Otherwise it returns DoNotCross. If either edge is
/// degenerate (A == B or C == D), the return value is MaybeCross if two vertices from
/// different edges are the same and DoNotCross otherwise.
///
/// Properties of CrossingSign:
///
///  (1) CrossingSign(b,a,c,d) == CrossingSign(a,b,c,d)
///  (2) CrossingSign(c,d,a,b) == CrossingSign(a,b,c,d)
///  (3) CrossingSign(a,b,c,d) == MaybeCross if a==c, a==d, b==c, b==d
///  (3) CrossingSign(a,b,c,d) == DoNotCross or MaybeCross if a==b or c==d
///
/// This method implements an exact, consistent perturbation model such that no three points
/// are ever considered to be collinear. This means that even if you have 4 points A, B, C, D
/// that lie exactly in a line (say, around the equator), C and D will be treated as being
/// slightly to one side or the other of AB. This is done in a way such that the results are
/// always consistent (see RobustSign).
pub fn crossing_sign(a: Point, b: Point, c: Point, d: Point) -> Crossing {
    let mut crosser = EdgeCrosser::new_chain(a, b, c);
    crosser.chain_crossing_sign(d)
}

/// Reports whether two edges "cross" in such a way that point-in-polygon containment tests
/// can be implemented by counting the number of edge crossings.
///
/// Given two edges AB and CD where at least two vertices are identical (i.e.
/// CrossingSign(a,b,c,d) == 0), the basic rule is that a "crossing" occurs if AB is
/// encountered after CD during a CCW sweep around the shared vertex starting from a fixed
/// reference point.
///
/// Useful properties of VertexCrossing (VC):
///
///  (1) VC(a,a,c,d) == VC(a,b,c,c) == false
///  (2) VC(a,b,a,b) == VC(a,b,b,a) == true
///  (3) VC(a,b,c,d) == VC(a,b,d,c) == VC(b,a,c,d) == VC(b,a,d,c)
///  (3) If exactly one of a,b equals one of c,d, then exactly one of
///  VC(a,b,c,d) and VC(c,d,a,b) is true
///
/// It is an error to call this method with 4 distinct vertices.
pub fn vertex_crossing(a: Point, b: Point, c: Point, d: Point) -> bool {
    // If A == B or C == D there is no intersection. We need to check this
    // case first in case 3 or more input points are identical.
    if a == b || c == d {
        return false;
    }

    // If any other pair of vertices is equal, there is a crossing if and only
    // if OrderedCCW indicates that the edge AB is further CCW around the
    // shared vertex O (either A or B) than the edge CD, starting from an
    // arbitrary fixed reference point.

    // Optimization: if AB=CD or AB=DC, we can avoid most of the calculations.
    if a == c {
        return (b == d) || ordered_ccw(a.reference_dir(), d, b, a);
    }
    if b == d {
        return ordered_ccw(b.reference_dir(), c, a, b);
    }
    if a == d {
        return (b == c) || ordered_ccw(a.reference_dir(), c, b, a);
    }
    if b == c {
        return ordered_ccw(b.reference_dir(), d, a, b);
    }

    false
}

/// A convenience function that calls CrossingSign to handle cases where all four vertices
/// are distinct, and VertexCrossing to handle cases where two or more vertices are the same.
/// This defines a crossing function such that point-in-polygon containment tests can be
/// implemented by simply counting edge crossings.
pub fn edge_or_vertex_crossing(a: Point, b: Point, c: Point, d: Point) -> bool {
    match crossing_sign(a, b, c, d) {
        Crossing::DoNotCross => false,
        Crossing::Cross => true,
        Crossing::MaybeCross => vertex_crossing(a, b, c, d),
    }
}

/// Reports if the angle ABC contains its vertex B. Containment is defined such that if
/// several polygons tile the region around a vertex, then exactly one of those polygons
/// contains that vertex. Returns false for degenerate angles of the form ABA.
///
/// Useful properties of AngleContainsVertex:
///
///  (1) AngleContainsVertex(a,b,a) == false
///  (2) AngleContainsVertex(a,b,c) == !AngleContainsVertex(c,b,a) unless a == c
///  (3) Given vertices v_1 ... v_k ordered cyclically CCW around vertex b,
///  AngleContainsVertex(v_{i+1}, b, v_i) is true for exactly one value of i.
///
/// REQUIRES: a != b && b != c
pub fn angle_contains_vertex(a: Point, b: Point, c: Point) -> bool {
    // A loop with consecutive vertices A, B, C contains vertex B if and only if
    // the fixed vector R = referenceDir(B) is contained by the wedge ABC.  The
    // wedge is closed at A and open at C, i.e. the point B is inside the loop
    // if A = R but not if C = R.
    //
    // Note that the test below is written so as to get correct results when the
    // angle ABC is degenerate. If A = C or C = R it returns false, and
    // otherwise if A = R it returns true.
    !ordered_ccw(b.reference_dir(), c, a, b)
}
