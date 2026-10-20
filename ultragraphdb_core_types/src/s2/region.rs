/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/region.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::s2::cap::Cap;
use crate::s2::cell::Cell;
use crate::s2::cellid::CellId;
use crate::s2::point::Point;
use crate::s2::rect::Rect;

/// A two-dimensional region on the unit sphere. The purpose of this interface is to allow
/// complex regions to be approximated as simpler regions. The interface is restricted to
/// methods that are useful for computing approximations. Implemented with static dispatch:
/// the RegionCoverer is generic over it.
pub trait Region {
    /// Returns a bounding spherical cap. This is not guaranteed to be exact.
    fn cap_bound(&self) -> Cap;

    /// Returns a bounding latitude-longitude rectangle that contains the region. The bounds
    /// are not guaranteed to be tight.
    fn rect_bound(&self) -> Rect;

    /// Reports whether the region completely contains the given region. It returns false if
    /// containment could not be determined.
    fn contains_cell(&self, c: &Cell) -> bool;

    /// Reports whether the region intersects the given cell or if intersection could not be
    /// determined. It returns false if the region does not intersect.
    fn intersects_cell(&self, c: &Cell) -> bool;

    /// Reports whether the region contains the given point or not. The point should be unit
    /// length, although some implementations may relax this restriction.
    fn contains_point(&self, p: Point) -> bool;

    /// Returns a small collection of CellIDs whose union covers the region. The cells are not
    /// sorted, may have redundancies (such as cells that contain other cells), and may cover
    /// much more area than necessary.
    ///
    /// This method is not intended for direct use by client code. Clients should typically
    /// use Covering, which has options to control the size and accuracy of the covering.
    /// Alternatively, if you want a fast covering and don't care about accuracy, consider
    /// calling FastCovering (which returns a cleaned-up version of the covering computed by
    /// this method).
    ///
    /// CellUnionBound implementations should attempt to return a small covering (ideally 4
    /// cells or fewer) that covers the region and can be computed quickly. The result is
    /// used by RegionCoverer as a starting point for further refinement.
    fn cell_union_bound(&self) -> Vec<CellId>;
}

impl Region for Cell {
    fn cap_bound(&self) -> Cap {
        Cell::cap_bound(self)
    }

    fn rect_bound(&self) -> Rect {
        Cell::rect_bound(self)
    }

    fn contains_cell(&self, c: &Cell) -> bool {
        Cell::contains_cell(self, c)
    }

    fn intersects_cell(&self, c: &Cell) -> bool {
        Cell::intersects_cell(self, c)
    }

    fn contains_point(&self, p: Point) -> bool {
        Cell::contains_point(self, p)
    }

    fn cell_union_bound(&self) -> Vec<CellId> {
        Cell::cell_union_bound(self)
    }
}

impl Region for crate::s2::cellunion::CellUnion {
    fn cap_bound(&self) -> Cap {
        crate::s2::cellunion::CellUnion::cap_bound(self)
    }

    fn rect_bound(&self) -> Rect {
        crate::s2::cellunion::CellUnion::rect_bound(self)
    }

    fn contains_cell(&self, c: &Cell) -> bool {
        crate::s2::cellunion::CellUnion::contains_cell(self, c)
    }

    fn intersects_cell(&self, c: &Cell) -> bool {
        crate::s2::cellunion::CellUnion::intersects_cell(self, c)
    }

    fn contains_point(&self, p: Point) -> bool {
        crate::s2::cellunion::CellUnion::contains_point(self, p)
    }

    fn cell_union_bound(&self) -> Vec<CellId> {
        crate::s2::cellunion::CellUnion::cell_union_bound(self)
    }
}
