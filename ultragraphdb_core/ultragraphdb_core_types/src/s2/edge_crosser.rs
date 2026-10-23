/*
 * Copyright 2017 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/edge_crosser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::s2::edge_crossings::{Crossing, vertex_crossing};
use crate::s2::point::Point;
use crate::s2::predicates::{Direction, expensive_sign, robust_sign, triage_sign};

/// Allows edges to be efficiently tested for intersection with a given fixed edge AB. It is
/// especially efficient when testing for intersection with an edge chain connecting vertices
/// v0, v1, v2, ...
#[derive(Clone, Copy, Debug)]
pub struct EdgeCrosser {
    a: Point,
    b: Point,

    // To reduce the number of calls to expensiveSign, we compute an
    // outward-facing tangent at A and B if necessary. If the plane
    // perpendicular to one of these tangents separates AB from CD (i.e., one
    // edge on each side) then there is no intersection.
    a_tangent: Point, // Outward-facing tangent at A.
    b_tangent: Point, // Outward-facing tangent at B.

    // The fields below are updated for each vertex in the chain.
    c: Point,       // Previous vertex in the vertex chain.
    acb: Direction, // The orientation of triangle ACB.
}

impl EdgeCrosser {
    /// Returns an EdgeCrosser with the fixed edge AB.
    pub fn new(a: Point, b: Point) -> EdgeCrosser {
        let norm = a.point_cross(b);
        EdgeCrosser {
            a,
            b,
            a_tangent: Point(a.0.cross(norm.0)),
            b_tangent: Point(norm.0.cross(b.0)),
            c: Point::default(),
            acb: Direction::Indeterminate,
        }
    }

    /// A convenience constructor that uses AB as the fixed edge, and C as the first vertex of
    /// the vertex chain (equivalent to calling RestartAt(c)).
    pub fn new_chain(a: Point, b: Point, c: Point) -> EdgeCrosser {
        let mut e = EdgeCrosser::new(a, b);
        e.restart_at(c);
        e
    }

    /// Reports whether the edge AB intersects the edge CD. If any two vertices from different
    /// edges are the same, returns MaybeCross. If either edge is degenerate (A == B or C == D),
    /// returns either DoNotCross or MaybeCross.
    pub fn crossing_sign(&mut self, c: Point, d: Point) -> Crossing {
        if c != self.c {
            self.restart_at(c);
        }
        self.chain_crossing_sign(d)
    }

    /// Reports whether if CrossingSign(c, d) > 0, or AB and CD share a vertex and
    /// VertexCrossing(a, b, c, d) is true.
    pub fn edge_or_vertex_crossing(&mut self, c: Point, d: Point) -> bool {
        if c != self.c {
            self.restart_at(c);
        }
        self.edge_or_vertex_chain_crossing(d)
    }

    /// Sets the current point of the edge crosser to be c. Call this method when your chain
    /// 'jumps' to a new place.
    pub fn restart_at(&mut self, c: Point) {
        self.c = c;
        self.acb = -triage_sign(self.a, self.b, self.c);
    }

    /// Like CrossingSign, but uses the last vertex passed to one of the crossing methods (or
    /// RestartAt) as the first vertex of the current edge.
    pub fn chain_crossing_sign(&mut self, d: Point) -> Crossing {
        // For there to be an edge crossing, the triangles ACB, CBD, BDA, DAC must
        // all be oriented the same way (CW or CCW). We keep the orientation of ACB
        // as part of our state. When each new point D arrives, we compute the
        // orientation of BDA and check whether it matches ACB. This checks whether
        // the points C and D are on opposite sides of the great circle through AB.

        // Recall that triageSign is invariant with respect to rotating its
        // arguments, i.e. ABD has the same orientation as BDA.
        let bda = triage_sign(self.a, self.b, d);
        if self.acb == -bda && bda != Direction::Indeterminate {
            // The most common case -- triangles have opposite orientations. Save the
            // current vertex D as the next vertex C, and also save the orientation of
            // the new triangle ACB (which is opposite to the current triangle BDA).
            self.c = d;
            self.acb = -bda;
            return Crossing::DoNotCross;
        }
        self.crossing_sign_slow(d, bda)
    }

    /// Like EdgeOrVertexCrossing, but uses the last vertex passed to one of the crossing
    /// methods (or RestartAt) as the first vertex of the current edge.
    pub fn edge_or_vertex_chain_crossing(&mut self, d: Point) -> bool {
        // We need to copy e.c since it is clobbered by ChainCrossingSign.
        let c = self.c;
        match self.chain_crossing_sign(d) {
            Crossing::DoNotCross => false,
            Crossing::Cross => true,
            Crossing::MaybeCross => vertex_crossing(self.a, self.b, c, d),
        }
    }

    /// Handles the slow path of CrossingSign. Go defers saving the current vertex D as the next
    /// vertex C and the orientation of the next triangle ACB (opposite to the current triangle
    /// BDA, as updated below); the deferred closure reads bda after any update.
    fn crossing_sign_slow(&mut self, d: Point, bda: Direction) -> Crossing {
        let mut bda = bda;
        let result = self.crossing_sign_slow_inner(d, &mut bda);
        self.c = d;
        self.acb = -bda;
        result
    }

    fn crossing_sign_slow_inner(&mut self, d: Point, bda: &mut Direction) -> Crossing {
        // At this point, a very common situation is that A,B,C,D are four points on
        // a line such that AB does not overlap CD. (For example, this happens when
        // a line or curve is sampled finely, or when geometry is constructed by
        // computing the union of S2CellIds.) Most of the time, we can determine
        // that AB and CD do not intersect using the two outward-facing
        // tangents at A and B (parallel to AB) and testing whether AB and CD are on
        // opposite sides of the plane perpendicular to one of these tangents. This
        // is moderately expensive but still much cheaper than expensiveSign.

        // The error in RobustCrossProd is insignificant. The maximum error in
        // the call to CrossProd (i.e., the maximum norm of the error vector) is
        // (0.5 + 1/sqrt(3)) * machineEpsilon64. The maximum error in each call to
        // DotProd below is machineEpsilon64. (There is also a small relative error
        // term that is insignificant because we are comparing the result against a
        // constant that is very close to zero.)
        let max_error = (1.5 + 1.0 / 3f64.sqrt()) * f64::EPSILON;
        if (self.c.0.dot(self.a_tangent.0) > max_error && d.0.dot(self.a_tangent.0) > max_error)
            || (self.c.0.dot(self.b_tangent.0) > max_error && d.0.dot(self.b_tangent.0) > max_error)
        {
            return Crossing::DoNotCross;
        }

        // Otherwise, eliminate the cases where two vertices from different edges are
        // equal. (These cases could be handled in the code below, but we would rather
        // avoid calling ExpensiveSign if possible.)
        if self.a == self.c || self.a == d || self.b == self.c || self.b == d {
            return Crossing::MaybeCross;
        }

        // Eliminate the cases where an input edge is degenerate. (Note that in
        // most cases, if CD is degenerate then this method is not even called
        // because acb and bda have different signs.)
        if self.a == self.b || self.c == d {
            return Crossing::DoNotCross;
        }

        // Otherwise it's time to break out the big guns.
        if self.acb == Direction::Indeterminate {
            self.acb = -expensive_sign(self.a, self.b, self.c);
        }
        if *bda == Direction::Indeterminate {
            *bda = expensive_sign(self.a, self.b, d);
        }

        if *bda != self.acb {
            return Crossing::DoNotCross;
        }

        let cbd = -robust_sign(self.c, d, self.b);
        if cbd != self.acb {
            return Crossing::DoNotCross;
        }
        let dac = robust_sign(self.c, d, self.a);
        if dac != self.acb {
            return Crossing::DoNotCross;
        }
        Crossing::Cross
    }
}
