/*
 * Copyright 2016 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/shapeindex.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::s2::cellid::CellId;
use crate::s2::edge_crosser::EdgeCrosser;
use crate::s2::point::Point;
use crate::s2::shape::Edge;
use crate::s2::stuv::face_uv_to_xyz;

/// Keeps track of which shapes in a given set contain a particular point (the focus). It
/// provides an efficient way to move the focus from one point to another and incrementally
/// update the set of shapes which contain it. We use this to compute which shapes contain the
/// center of every CellID in the index, by advancing the focus from one cell center to the
/// next.
///
/// Initially the focus is at the start of the CellID space-filling curve. We then visit all
/// the cells that are being added to the ShapeIndex in increasing order of CellID. For each
/// cell, we draw two edges: one from the entry vertex to the center, and another from the
/// center to the exit vertex (where entry and exit refer to the points where the
/// space-filling curve enters and exits the cell). By counting edge crossings we can
/// incrementally compute which shapes contain the cell center. Note that the same set of
/// shapes will always contain the exit point of one cell and the entry point of the next
/// cell in the index, because either (a) these two points are actually the same, or (b) the
/// intervening cells in CellID order are all empty, and therefore there are no edge
/// crossings if we follow this path from one cell to the other.
///
/// In C++, this is S2ShapeIndex::InteriorTracker.
pub(crate) struct Tracker {
    pub(crate) is_active: bool,
    a: Point,
    b: Point,
    next_cell_id: CellId,
    crosser: EdgeCrosser,
    pub(crate) shape_ids: Vec<i32>,
}

impl Tracker {
    /// Returns a new tracker with the appropriate defaults.
    pub(crate) fn new() -> Tracker {
        // As shapes are added, we compute which ones contain the start of the
        // CellID space-filling curve by drawing an edge from OriginPoint to this
        // point and counting how many shape edges cross this edge.
        let origin = tracker_origin();
        let mut t = Tracker {
            is_active: false,
            a: Point::default(),
            b: origin,
            next_cell_id: CellId::from_face(0).child_begin_at_level(crate::s2::cellid::MAX_LEVEL),
            crosser: EdgeCrosser::new(origin, origin),
            shape_ids: Vec::new(),
        };
        t.draw_to(Point(face_uv_to_xyz(0, -1.0, -1.0).normalize())); // CellID curve start
        t
    }

    /// Returns the current focus point of the tracker.
    pub(crate) fn focus(&self) -> Point {
        self.b
    }

    /// Adds a shape whose interior should be tracked. containsFocus indicates whether the
    /// current focus point is inside the shape.
    pub(crate) fn add_shape(&mut self, shape_id: i32, contains_focus: bool) {
        self.is_active = true;
        if contains_focus {
            self.toggle_shape(shape_id);
        }
    }

    /// Moves the focus of the tracker to the given point. This method should only be used
    /// when it is known that there are no edge crossings between the old and new focus
    /// locations; otherwise use drawTo.
    pub(crate) fn move_to(&mut self, b: Point) {
        self.b = b;
    }

    /// Moves the focus of the tracker to the given point. After this method is called,
    /// testEdge should be called with all edges that may cross the line segment between the
    /// old and new focus locations.
    pub(crate) fn draw_to(&mut self, b: Point) {
        self.a = self.b;
        self.b = b;
        self.crosser = EdgeCrosser::new(self.a, self.b);
    }

    /// Checks if the given edge crosses the current edge, and if so, then toggle the state of
    /// the given shapeID. This requires shape to have an interior.
    pub(crate) fn test_edge(&mut self, shape_id: i32, edge: Edge) {
        if self.crosser.edge_or_vertex_crossing(edge.v0, edge.v1) {
            self.toggle_shape(shape_id);
        }
    }

    /// Used to indicate that the last argument to moveTo or drawTo was the entry vertex of the
    /// given CellID, i.e. the tracker is positioned at the start of this cell. By using this
    /// method together with atCellID, the caller can avoid calling moveTo in cases where the
    /// exit vertex of the previous cell is the same as the entry vertex of the current cell.
    pub(crate) fn set_next_cell_id(&mut self, next_cell_id: CellId) {
        self.next_cell_id = next_cell_id.range_min();
    }

    /// Reports if the focus is already at the entry vertex of the given CellID (provided that
    /// the caller calls setNextCellID as each cell is processed).
    pub(crate) fn at_cell_id(&self, cellid: CellId) -> bool {
        cellid.range_min() == self.next_cell_id
    }

    /// Adds or removes the given shapeID from the set of IDs it is tracking.
    fn toggle_shape(&mut self, shape_id: i32) {
        // Most shapeIDs slices are small, so special case the common steps.

        // If there is nothing here, add it.
        if self.shape_ids.is_empty() {
            self.shape_ids.push(shape_id);
            return;
        }

        // If it's the first element, drop it from the slice.
        if self.shape_ids[0] == shape_id {
            self.shape_ids.remove(0);
            return;
        }

        for i in 0..self.shape_ids.len() {
            let s = self.shape_ids[i];
            if s < shape_id {
                continue;
            }

            // If it's in the set, cut it out.
            if s == shape_id {
                self.shape_ids.remove(i);
                return;
            }

            // We've got to a point in the slice where we should be inserted.
            // (the given shapeID is now less than the current positions id.)
            self.shape_ids.insert(i, shape_id);
            return;
        }

        // We got to the end and didn't find it, so add it to the list.
        self.shape_ids.push(shape_id);
    }
}

/// Returns the initial focus point when the tracker is created (corresponding to the start
/// of the CellID space-filling curve).
fn tracker_origin() -> Point {
    // The start of the S2CellId space-filling curve.
    Point(face_uv_to_xyz(0, -1.0, -1.0).normalize())
}
