/*
 * Copyright 2015 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/loop.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::sync::OnceLock;

use crate::s2::cap::Cap;
use crate::s2::cell::Cell;
use crate::s2::cellid::CellId;
use crate::s2::edge_clipping::{clip_to_padded_face, edge_intersects_rect};
use crate::s2::edge_crosser::EdgeCrosser;
use crate::s2::edge_crossings::angle_contains_vertex;
use crate::s2::matrix3x3::{Matrix3x3, get_frame};
use crate::s2::point::{Point, regular_points_for_frame};
use crate::s2::r1::R1Interval;
use crate::s2::r3::R3Vector;
use crate::s2::rect::Rect;
use crate::s2::rect_bounder::{RectBounder, expand_for_subregions};
use crate::s2::region::Region;
use crate::s2::s1::{Angle, S1Interval};
use crate::s2::shape::{Edge, ReferencePoint, Shape};
use crate::s2::shapeindex::{CellRelation, ShapeIndex, ShapeIndexIterator};
use core::f64::consts::FRAC_PI_2;

/// The point used for the special empty loop.
const EMPTY_LOOP_POINT: Point = Point(R3Vector::new(0.0, 0.0, 1.0));
/// The point used for the special full loop.
const FULL_LOOP_POINT: Point = Point(R3Vector::new(0.0, 0.0, -1.0));

/// Go's constant `faceClipErrorUVCoord + intersectsRectErrorUVDist`, rounded once from the
/// exact sum.
const BOUNDARY_MAX_ERROR: f64 = f64::from_bits(0x3ce5369480174810);

/// For loops with at most this many vertices ContainsPoint counts crossings directly.
const MAX_BRUTE_FORCE_VERTICES: usize = 32;

/// A simple spherical polygon. It consists of a sequence of vertices where the first vertex
/// is implicitly connected to the last. All loops are defined to have a CCW orientation,
/// i.e. the interior of the loop is on the left side of the edges. This implies that a
/// clockwise loop enclosing a small area is interpreted to be a CCW loop enclosing a very
/// large area.
///
/// Loops are not allowed to have any duplicate vertices (whether adjacent or not).
/// Non-adjacent edges are not allowed to intersect, and furthermore edges of length 180
/// degrees are not allowed (i.e., adjacent vertices cannot be antipodal). Loops must have at
/// least 3 vertices (except for the "empty" and "full" loops discussed below).
///
/// There are two special loops: the "empty" loop contains no points and the "full" loop
/// contains all points. These loops do not have any edges, but to preserve the invariant
/// that every loop can be represented as a vertex chain, they are defined as having exactly
/// one vertex each (see EmptyLoop and FullLoop).
///
/// The edge index is built on first use, as Go's ShapeIndex applies its pending update on
/// the first query, and the loop reproduces the observable effects of that laziness: until
/// the index is built ContainsPoint first tests the bound, and an Invert after the index was
/// built leaves Go's index empty (its Reset keeps the count of shapes already applied), which
/// this port reproduces.
#[derive(Clone, Debug)]
pub struct Loop {
    vertices: Vec<Point>,

    // originInside keeps a precomputed value whether this loop contains the origin
    // versus computing from the set of vertices every time.
    origin_inside: bool,

    // bound is a conservative bound on all points contained by this loop.
    // If l.ContainsPoint(P), then l.bound.ContainsPoint(P).
    bound: Rect,

    // Since bound is not exact, it is possible that a loop A contains
    // another loop B whose bounds are slightly larger. subregionBound
    // has been expanded sufficiently to account for this error, i.e.
    // if A.Contains(B), then A.subregionBound.Contains(B.bound).
    subregion_bound: Rect,

    // Whether the loop has been added to its index (Go returns from
    // initOriginAndBound before adding a loop of 0 or 2 vertices).
    index_has_shape: bool,

    // Whether Go's index would come out empty when next built: an Invert after
    // the index was built.
    index_poisoned: bool,

    // The spatial index for this Loop, built on first use.
    index: OnceLock<ShapeIndex>,
}

impl Loop {
    /// Constructs a loop from the given points.
    pub fn from_points(pts: Vec<Point>) -> Loop {
        let mut l = Loop {
            vertices: pts,
            origin_inside: false,
            bound: Rect::new(R1Interval::new(0.0, 0.0), S1Interval::new(0.0, 0.0)),
            subregion_bound: Rect::new(R1Interval::new(0.0, 0.0), S1Interval::new(0.0, 0.0)),
            index_has_shape: false,
            index_poisoned: false,
            index: OnceLock::new(),
        };
        l.init_origin_and_bound();
        l
    }

    /// Returns a special "empty" loop.
    pub fn empty() -> Loop {
        Loop::from_points(vec![EMPTY_LOOP_POINT])
    }

    /// Returns a special "full" loop.
    pub fn full() -> Loop {
        Loop::from_points(vec![FULL_LOOP_POINT])
    }

    /// Creates a loop with the given number of vertices, all located on a circle of the
    /// specified radius around the given center.
    pub fn regular(center: Point, radius: Angle, num_vertices: usize) -> Loop {
        Loop::regular_for_frame(&get_frame(center), radius, num_vertices)
    }

    /// Creates a loop centered around the z-axis of the given coordinate frame, with the
    /// first vertex in the direction of the positive x-axis.
    pub(crate) fn regular_for_frame(frame: &Matrix3x3, radius: Angle, num_vertices: usize) -> Loop {
        Loop::from_points(regular_points_for_frame(frame, radius, num_vertices))
    }

    /// Sets the origin containment for the given point and then calls the initialization for
    /// the bounds objects and the internal index.
    fn init_origin_and_bound(&mut self) {
        if self.vertices.len() < 3 {
            // Check for the special "empty" and "full" loops (which have one vertex).
            if !self.is_empty_or_full() {
                self.origin_inside = false;
                return;
            }

            // This is the special empty or full loop, so the origin depends on if
            // the vertex is in the southern hemisphere or not.
            self.origin_inside = self.vertices[0].0.z < 0.0;
        } else {
            // The brute force point containment algorithm works by counting edge
            // crossings starting at a fixed reference point (chosen as OriginPoint()
            // for historical reasons).  Loop initialization would be more efficient
            // if we used a loop vertex such as vertex(0) as the reference point
            // instead, however making this change would be a lot of work because
            // originInside is currently part of the Encode() format.
            //
            // In any case, we initialize originInside by first guessing that it is
            // outside, and then seeing whether we get the correct containment result
            // for vertex 1.  If the result is incorrect, the origin must be inside
            // the loop instead.  Note that the Loop is not necessarily valid and so
            // we need to check the requirements of AngleContainsVertex first.
            let v = &self.vertices;
            let v1_inside = v[0] != v[1] && v[2] != v[1] && angle_contains_vertex(v[0], v[1], v[2]);

            // initialize before calling ContainsPoint
            self.origin_inside = false;

            // Note that ContainsPoint only does a bounds check once initIndex
            // has been called, so it doesn't matter that bound is undefined here.
            if v1_inside != self.brute_force_contains_point(self.vertices[1]) {
                self.origin_inside = true;
            }
        }

        // We *must* call initBound before initializing the index, because
        // initBound calls ContainsPoint which does a bounds check before using
        // the index.
        self.init_bound();

        // Create a new index and add us to it.
        self.index = OnceLock::new();
        self.index_has_shape = true;
    }

    /// Sets up the approximate bounding Rects for this loop. Go's ContainsPoint counts
    /// crossings here, since the index holds no shape yet.
    fn init_bound(&mut self) {
        if self.vertices.is_empty() {
            *self = Loop::empty();
            return;
        }
        // Check for the special "empty" and "full" loops.
        if self.is_empty_or_full() {
            self.bound = if self.is_empty() {
                Rect::empty()
            } else {
                Rect::full()
            };
            self.subregion_bound = self.bound;
            return;
        }

        // The bounding rectangle of a loop is not necessarily the same as the
        // bounding rectangle of its vertices. First, the maximal latitude may be
        // attained along the interior of an edge. Second, the loop may wrap
        // entirely around the sphere (e.g. a loop that defines two revolutions of a
        // candy-cane stripe). Third, the loop may include one or both poles.
        // Note that a small clockwise loop near the equator contains both poles.
        let mut bounder = RectBounder::new();
        for i in 0..=self.vertices.len() {
            // add vertex 0 twice
            bounder.add_point(self.vertex(i));
        }
        let mut b = bounder.rect_bound();

        if self.brute_force_contains_point(Point(R3Vector::new(0.0, 0.0, 1.0))) {
            b = Rect::new(R1Interval::new(b.lat.lo, FRAC_PI_2), S1Interval::full());
        }
        // If a loop contains the south pole, then either it wraps entirely
        // around the sphere (full longitude range), or it also contains the
        // north pole in which case b.Lng.IsFull() due to the test above.
        // Either way, we only need to do the south pole containment test if
        // b.Lng.IsFull().
        if b.lng.is_full() && self.brute_force_contains_point(Point(R3Vector::new(0.0, 0.0, -1.0)))
        {
            b.lat.lo = -FRAC_PI_2;
        }
        self.bound = b;
        self.subregion_bound = expand_for_subregions(self.bound);
    }

    /// Returns the index, building it on first use. An index Go would leave empty (see the
    /// type documentation) is built empty.
    fn index(&self) -> &ShapeIndex {
        self.index.get_or_init(|| {
            if self.index_has_shape && !self.index_poisoned {
                ShapeIndex::build(self)
            } else {
                ShapeIndex::empty()
            }
        })
    }

    /// Reports whether Go's index would report itself fresh: built, or holding no shape.
    fn index_is_fresh(&self) -> bool {
        !self.index_has_shape || self.index.get().is_some()
    }

    /// Reports true if this loop contains s2.OriginPoint().
    pub fn contains_origin(&self) -> bool {
        self.origin_inside
    }

    /// Returns the number of edges in this shape.
    pub fn num_edges(&self) -> usize {
        if self.is_empty_or_full() {
            return 0;
        }
        self.vertices.len()
    }

    /// Reports true if this is the special empty loop that contains no points.
    pub fn is_empty(&self) -> bool {
        self.is_empty_or_full() && !self.contains_origin()
    }

    /// Reports true if this is the special full loop that contains all points.
    pub fn is_full(&self) -> bool {
        self.is_empty_or_full() && self.contains_origin()
    }

    /// Reports true if this loop is either the "empty" or "full" special loops.
    fn is_empty_or_full(&self) -> bool {
        self.vertices.len() == 1
    }

    /// Returns the vertices in the loop.
    pub fn vertices(&self) -> &[Point] {
        &self.vertices
    }

    /// Returns the number of vertices in this loop.
    pub fn num_vertices(&self) -> usize {
        self.vertices.len()
    }

    /// Returns a tight bounding rectangle. If the loop contains the point, the bound also
    /// contains it.
    pub fn rect_bound(&self) -> Rect {
        self.bound
    }

    /// Returns a bounding cap that may have more padding than the corresponding RectBound.
    /// The bound is conservative such that if the loop contains a point P, the bound also
    /// contains it.
    pub fn cap_bound(&self) -> Cap {
        self.bound.cap_bound()
    }

    /// Returns the vertex for the given index. For convenience, the vertex indices wrap
    /// automatically for methods that do index math such as Edge. i.e., Vertex(NumEdges() + n)
    /// is the same as Vertex(n).
    pub fn vertex(&self, i: usize) -> Point {
        self.vertices[i % self.vertices.len()]
    }

    /// Reports if the given point is contained by this loop. This method does not use the
    /// ShapeIndex, so it is only preferable below a certain size of loop.
    fn brute_force_contains_point(&self, p: Point) -> bool {
        let origin = Point::origin();
        let mut inside = self.origin_inside;
        let mut crosser = EdgeCrosser::new_chain(origin, p, self.vertex(0));
        for i in 1..=self.vertices.len() {
            // add vertex 0 twice
            inside = inside != crosser.edge_or_vertex_chain_crossing(self.vertex(i));
        }
        inside
    }

    /// Returns true if the loop contains the point.
    pub fn contains_point(&self, p: Point) -> bool {
        if !self.index_is_fresh() && !self.bound.contains_point(p) {
            return false;
        }

        // For small loops it is faster to just check all the crossings.  We also
        // use this method during loop initialization because InitOriginAndBound()
        // calls Contains() before InitIndex().  Otherwise, we keep track of the
        // number of calls to Contains() and only build the index when enough calls
        // have been made so that we think it is worth the effort.
        if !self.index_has_shape || self.vertices.len() <= MAX_BRUTE_FORCE_VERTICES {
            return self.brute_force_contains_point(p);
        }

        // Otherwise, look up the point in the index.
        let mut it = self.index().iterator();
        if !it.locate_point(p) {
            return false;
        }
        self.iterator_contains_point(&it, p)
    }

    /// Reports whether the given Cell is contained by this Loop.
    pub fn contains_cell(&self, target: &Cell) -> bool {
        let mut it = self.index().iterator();
        let relation = it.locate_cell_id(target.id());

        // If "target" is disjoint from all index cells, it is not contained.
        // Similarly, if "target" is subdivided into one or more index cells then it
        // is not contained, since index cells are subdivided only if they (nearly)
        // intersect a sufficient number of edges.  (But note that if "target" itself
        // is an index cell then it may be contained, since it could be a cell with
        // no edges in the loop interior.)
        if relation != CellRelation::Indexed {
            return false;
        }

        // Otherwise check if any edges intersect "target".
        if self.boundary_approx_intersects(&it, target) {
            return false;
        }

        // Otherwise check if the loop contains the center of "target".
        self.iterator_contains_point(&it, target.center())
    }

    /// Reports whether this Loop intersects the given cell.
    pub fn intersects_cell(&self, target: &Cell) -> bool {
        let mut it = self.index().iterator();
        let relation = it.locate_cell_id(target.id());

        // If target does not overlap any index cell, there is no intersection.
        if relation == CellRelation::Disjoint {
            return false;
        }
        // If target is subdivided into one or more index cells, there is an
        // intersection to within the ShapeIndex error bound (see Contains).
        if relation == CellRelation::Subdivided {
            return true;
        }
        // If target is an index cell, there is an intersection because index cells
        // are created only if they have at least one edge or they are entirely
        // contained by the loop.
        if it.cell_id() == target.id() {
            return true;
        }
        // Otherwise check if any edges intersect target.
        if self.boundary_approx_intersects(&it, target) {
            return true;
        }
        // Otherwise check if the loop contains the center of target.
        self.iterator_contains_point(&it, target.center())
    }

    /// Computes a covering of the Loop.
    pub fn cell_union_bound(&self) -> Vec<CellId> {
        self.cap_bound().cell_union_bound()
    }

    /// Reports if the loop's boundary intersects target. It may also return true when the
    /// loop boundary does not intersect target but some edge comes within the worst-case
    /// error tolerance.
    ///
    /// This requires that it.Locate(target) returned Indexed.
    fn boundary_approx_intersects(&self, it: &ShapeIndexIterator<'_>, target: &Cell) -> bool {
        let a_clipped = it
            .index_cell()
            .find_by_shape_id(0)
            .expect("index cell holds shape 0");

        // If there are no edges, there is no intersection.
        if a_clipped.edges.is_empty() {
            return false;
        }

        // We can save some work if target is the index cell itself.
        if it.cell_id() == target.id() {
            return true;
        }

        // Otherwise check whether any of the edges intersect target.
        let bound = target.bound_uv().expanded_by_margin(BOUNDARY_MAX_ERROR);
        for &ai in &a_clipped.edges {
            if let Some((v0, v1)) = clip_to_padded_face(
                self.vertex(ai),
                self.vertex(ai + 1),
                target.face(),
                BOUNDARY_MAX_ERROR,
            ) && edge_intersects_rect(v0, v1, bound)
            {
                return true;
            }
        }
        false
    }

    /// Reports if the iterator that is positioned at the ShapeIndexCell that may contain p,
    /// contains the point p.
    fn iterator_contains_point(&self, it: &ShapeIndexIterator<'_>, p: Point) -> bool {
        // Test containment by drawing a line segment from the cell center to the
        // given point and counting edge crossings.
        let a_clipped = it
            .index_cell()
            .find_by_shape_id(0)
            .expect("index cell holds shape 0");
        let mut inside = a_clipped.contains_center;
        if !a_clipped.edges.is_empty() {
            let center = it.center();
            let mut crosser = EdgeCrosser::new(center, p);
            let mut ai_prev: i64 = -2;
            for &ai in &a_clipped.edges {
                if ai as i64 != ai_prev + 1 {
                    crosser.restart_at(self.vertex(ai));
                }
                ai_prev = ai as i64;
                inside = inside != crosser.edge_or_vertex_chain_crossing(self.vertex(ai + 1));
            }
        }
        inside
    }

    /// Reverses the order of the loop vertices, effectively complementing the region
    /// represented by the loop. For example, the loop ABCD (with edges AB, BC, CD, DA) becomes
    /// the loop DCBA (with edges DC, CB, BA, AD). Notice that the last edge is the same in
    /// both cases except that its direction has been reversed.
    pub fn invert(&mut self) {
        // Go's index.Reset keeps the position up to which shapes were applied, so once the
        // index was built, rebuilding it after this reset adds no shape.
        if self.index_has_shape && self.index.get().is_some() {
            self.index_poisoned = true;
        }
        self.index = OnceLock::new();
        self.index_has_shape = false;
        if self.is_empty_or_full() {
            if self.is_full() {
                self.vertices[0] = EMPTY_LOOP_POINT;
            } else {
                self.vertices[0] = FULL_LOOP_POINT;
            }
        } else {
            // For non-special loops, reverse the slice of vertices.
            self.vertices.reverse();
        }

        // originInside must be set correctly before building the ShapeIndex.
        self.origin_inside = !self.origin_inside;
        if self.bound.lat.lo > -FRAC_PI_2 && self.bound.lat.hi < FRAC_PI_2 {
            // The complement of this loop contains both poles.
            self.bound = Rect::full();
            self.subregion_bound = self.bound;
        } else {
            self.init_bound();
        }
        self.index_has_shape = true;
    }
}

impl Shape for Loop {
    fn num_edges(&self) -> usize {
        Loop::num_edges(self)
    }

    fn edge(&self, i: usize) -> Edge {
        Edge {
            v0: self.vertex(i),
            v1: self.vertex(i + 1),
        }
    }

    fn dimension(&self) -> i32 {
        2
    }

    fn reference_point(&self) -> ReferencePoint {
        ReferencePoint::origin(self.origin_inside)
    }
}

impl Region for Loop {
    fn cap_bound(&self) -> Cap {
        Loop::cap_bound(self)
    }

    fn rect_bound(&self) -> Rect {
        Loop::rect_bound(self)
    }

    fn contains_cell(&self, c: &Cell) -> bool {
        Loop::contains_cell(self, c)
    }

    fn intersects_cell(&self, c: &Cell) -> bool {
        Loop::intersects_cell(self, c)
    }

    fn contains_point(&self, p: Point) -> bool {
        Loop::contains_point(self, p)
    }

    fn cell_union_bound(&self) -> Vec<CellId> {
        Loop::cell_union_bound(self)
    }
}
