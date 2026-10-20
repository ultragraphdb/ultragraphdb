/*
 * Copyright 2016 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/paddedcell.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::s2::cellid::{
    CellId, IJ_TO_POS, INVERT_MASK, MAX_LEVEL, POS_TO_IJ, POS_TO_ORIENTATION, SWAP_MASK,
    cell_id_from_face_ij, ij_level_to_bound_uv, size_ij,
};
use crate::s2::point::Point;
use crate::s2::r1::R1Interval;
use crate::s2::r2::R2Rect;
use crate::s2::stuv::{face_si_ti_to_xyz, si_ti_to_st, st_to_ij, st_to_uv, uv_to_st};

/// Represents a Cell whose (u,v)-range has been expanded on all sides by a given amount of
/// "padding". Unlike Cell, its methods and representation are optimized for clipping edges
/// against Cell boundaries to determine which cells are intersected by a given set of edges.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PaddedCell {
    pub(crate) id: CellId,
    padding: f64,
    pub(crate) bound: R2Rect,
    middle: R2Rect, // A rect in (u, v)-space that belongs to all four children.
    i_lo: i64,      // Minimum (i,j)-coordinates of this cell before padding
    j_lo: i64,
    orientation: usize, // Hilbert curve orientation of this cell.
    level: i64,
}

impl PaddedCell {
    /// Constructs a padded cell with the given padding.
    pub(crate) fn from_cell_id(id: CellId, padding: f64) -> PaddedCell {
        let mut p = PaddedCell {
            id,
            padding,
            bound: R2Rect::default(),
            middle: R2Rect::empty(),
            i_lo: 0,
            j_lo: 0,
            orientation: 0,
            level: 0,
        };

        // Fast path for constructing a top-level face (the most common case).
        if id.is_face() {
            let limit = padding + 1.0;
            p.bound = R2Rect::new(
                R1Interval::new(-limit, limit),
                R1Interval::new(-limit, limit),
            );
            p.middle = R2Rect::new(
                R1Interval::new(-padding, padding),
                R1Interval::new(-padding, padding),
            );
            p.orientation = id.face() & 1;
            return p;
        }

        let (_, i, j, o) = id.face_ij_orientation();
        p.i_lo = i;
        p.j_lo = j;
        p.orientation = o;
        p.level = id.level();
        p.bound = ij_level_to_bound_uv(p.i_lo, p.j_lo, p.level).expanded_by_margin(padding);
        let ij_size = size_ij(p.level);
        p.i_lo &= -ij_size;
        p.j_lo &= -ij_size;
        p.middle = p.compute_middle();

        p
    }

    /// Constructs a new padded cell that is a child of the given parent.
    pub(crate) fn from_parent_ij(parent: &PaddedCell, i: usize, j: usize) -> PaddedCell {
        // Compute the position and orientation of the child incrementally from the
        // orientation of the parent.
        let pos = IJ_TO_POS[parent.orientation][2 * i + j];

        let mut p = PaddedCell {
            id: parent.id.children()[pos],
            padding: parent.padding,
            bound: parent.bound,
            orientation: parent.orientation ^ POS_TO_ORIENTATION[pos],
            level: parent.level + 1,
            middle: R2Rect::empty(),
            i_lo: 0,
            j_lo: 0,
        };

        let ij_size = size_ij(p.level);
        p.i_lo = parent.i_lo + i as i64 * ij_size;
        p.j_lo = parent.j_lo + j as i64 * ij_size;

        // For each child, one corner of the bound is taken directly from the parent
        // while the diagonally opposite corner is taken from middle().
        let middle = parent.middle;
        if i == 1 {
            p.bound.x.lo = middle.x.lo;
        } else {
            p.bound.x.hi = middle.x.hi;
        }
        if j == 1 {
            p.bound.y.lo = middle.y.lo;
        } else {
            p.bound.y.hi = middle.y.hi;
        }
        if p.level < MAX_LEVEL {
            p.middle = p.compute_middle();
        }

        p
    }

    /// Returns the level this cell is at.
    pub(crate) fn level(&self) -> i64 {
        self.level
    }

    /// Returns the center of this cell.
    pub(crate) fn center(&self) -> Point {
        let ij_size = size_ij(self.level);
        let si = (2 * self.i_lo + ij_size) as u32;
        let ti = (2 * self.j_lo + ij_size) as u32;
        Point(face_si_ti_to_xyz(self.id.face(), si, ti).0.normalize())
    }

    /// Returns the rectangle in the middle of this cell that belongs to all four of its
    /// children in (u,v)-space. Go computes it lazily on first use; it is computed here when
    /// the cell is built, which yields the same value.
    pub(crate) fn middle(&self) -> R2Rect {
        self.middle
    }

    fn compute_middle(&self) -> R2Rect {
        let ij_size = size_ij(self.level);
        let u = st_to_uv(si_ti_to_st((2 * self.i_lo + ij_size) as u32));
        let v = st_to_uv(si_ti_to_st((2 * self.j_lo + ij_size) as u32));
        R2Rect::new(
            R1Interval::new(u - self.padding, u + self.padding),
            R1Interval::new(v - self.padding, v + self.padding),
        )
    }

    /// Returns the (i,j) coordinates for the child cell at the given traversal position. The
    /// traversal position corresponds to the order in which child cells are visited by the
    /// Hilbert curve.
    pub(crate) fn child_ij(&self, pos: usize) -> (usize, usize) {
        let ij = POS_TO_IJ[self.orientation][pos];
        (ij >> 1, ij & 1)
    }

    /// Returns the vertex where the space-filling curve enters this cell.
    pub(crate) fn entry_vertex(&self) -> Point {
        // The curve enters at the (0,0) vertex unless the axis directions are
        // reversed, in which case it enters at the (1,1) vertex.
        let mut i = self.i_lo;
        let mut j = self.j_lo;
        if self.orientation & INVERT_MASK != 0 {
            let ij_size = size_ij(self.level);
            i += ij_size;
            j += ij_size;
        }
        Point(
            face_si_ti_to_xyz(self.id.face(), (2 * i) as u32, (2 * j) as u32)
                .0
                .normalize(),
        )
    }

    /// Returns the vertex where the space-filling curve exits this cell.
    pub(crate) fn exit_vertex(&self) -> Point {
        // The curve exits at the (1,0) vertex unless the axes are swapped or
        // inverted but not both, in which case it exits at the (0,1) vertex.
        let mut i = self.i_lo;
        let mut j = self.j_lo;
        let ij_size = size_ij(self.level);
        if self.orientation == 0 || self.orientation == SWAP_MASK + INVERT_MASK {
            i += ij_size;
        } else {
            j += ij_size;
        }
        Point(
            face_si_ti_to_xyz(self.id.face(), (2 * i) as u32, (2 * j) as u32)
                .0
                .normalize(),
        )
    }

    /// Returns the smallest CellID that contains all descendants of this padded cell whose
    /// bounds intersect the given rect. For algorithms that use recursive subdivision to find
    /// the cells that intersect a particular object, this method can be used to skip all of
    /// the initial subdivision steps where only one child needs to be expanded.
    ///
    /// Note that this method is not the same as returning the smallest cell that contains
    /// the intersection of this cell with rect. Because of the padding, even if one child
    /// completely contains rect it is still possible that a neighboring child may also
    /// intersect the given rect.
    ///
    /// The provided Rect must intersect the bounds of this cell.
    pub(crate) fn shrink_to_fit(&self, rect: R2Rect) -> CellId {
        // Quick rejection test: if rect contains the center of this cell along
        // either axis, then no further shrinking is possible.
        if self.level == 0 {
            // Fast path (most calls to this function start with a face cell).
            if rect.x.contains(0.0) || rect.y.contains(0.0) {
                return self.id;
            }
        }

        let ij_size = size_ij(self.level);
        if rect
            .x
            .contains(st_to_uv(si_ti_to_st((2 * self.i_lo + ij_size) as u32)))
            || rect
                .y
                .contains(st_to_uv(si_ti_to_st((2 * self.j_lo + ij_size) as u32)))
        {
            return self.id;
        }

        // Otherwise we expand rect by the given padding on all sides and find
        // the range of coordinates that it spans along the i- and j-axes. We then
        // compute the highest bit position at which the min and max coordinates
        // differ. This corresponds to the first cell level at which at least two
        // children intersect rect.

        // Increase the padding to compensate for the error in uvToST.
        // (The constant below is a provable upper bound on the additional error.)
        let padded = rect.expanded_by_margin(self.padding + 1.5 * f64::EPSILON);
        let mut i_min = self.i_lo; // Min i- or j- coordinate spanned by padded
        let mut j_min = self.j_lo;

        if i_min < st_to_ij(uv_to_st(padded.x.lo)) {
            i_min = st_to_ij(uv_to_st(padded.x.lo));
        }
        let a = self.i_lo + ij_size - 1;
        let b = st_to_ij(uv_to_st(padded.x.hi));
        let i_xor = if a <= b { i_min ^ a } else { i_min ^ b };

        if j_min < st_to_ij(uv_to_st(padded.y.lo)) {
            j_min = st_to_ij(uv_to_st(padded.y.lo));
        }
        let a = self.j_lo + ij_size - 1;
        let b = st_to_ij(uv_to_st(padded.y.hi));
        let j_xor = if a <= b { j_min ^ a } else { j_min ^ b };

        // Compute the highest bit position where the two i- or j-endpoints differ,
        // and then choose the cell level that includes both of these endpoints. So
        // if both pairs of endpoints are equal we choose MaxLevel; if they differ
        // only at bit 0, we choose (MaxLevel - 1), and so on.
        let level_msb = (((i_xor | j_xor) << 1) + 1) as u64;
        let level = MAX_LEVEL - (64 - level_msb.leading_zeros() as i64 - 1);
        if level <= self.level {
            return self.id;
        }

        cell_id_from_face_ij(self.id.face(), i_min, j_min).parent(level)
    }
}
