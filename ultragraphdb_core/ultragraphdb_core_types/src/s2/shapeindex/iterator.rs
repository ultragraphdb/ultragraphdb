/*
 * Copyright 2016 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/shapeindex.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use super::{ShapeIndex, ShapeIndexCell};
use crate::s2::cellid::{CellId, cell_id_from_point};
use crate::s2::point::Point;

/// The possible relationships between a target cell and the cells of the ShapeIndex. If the
/// target is an index cell or is contained by an index cell, it is Indexed. If the target is
/// subdivided into one or more index cells, it is Subdivided. Otherwise it is Disjoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CellRelation {
    Indexed,
    Subdivided,
    Disjoint,
}

/// An iterator that provides low-level access to the cells of the index. Cells are returned
/// in increasing order of CellID.
pub(crate) struct ShapeIndexIterator<'a> {
    index: &'a ShapeIndex,
    position: usize,
    id: CellId,
    cell: Option<&'a ShapeIndexCell>,
}

impl<'a> ShapeIndexIterator<'a> {
    /// Creates an iterator positioned at the beginning of the index.
    pub(crate) fn new(index: &'a ShapeIndex) -> ShapeIndexIterator<'a> {
        let mut it = ShapeIndexIterator {
            index,
            position: 0,
            id: CellId::SENTINEL,
            cell: None,
        };
        it.refresh();
        it
    }

    /// Returns the CellID of the current index cell. If done, a value larger than any valid
    /// CellID is returned.
    pub(crate) fn cell_id(&self) -> CellId {
        self.id
    }

    /// Returns the current index cell.
    pub(crate) fn index_cell(&self) -> &'a ShapeIndexCell {
        self.cell
            .expect("iterator is not positioned at an index cell")
    }

    /// Returns the Point at the center of the current position of the iterator.
    pub(crate) fn center(&self) -> Point {
        self.cell_id().point()
    }

    /// Advances the iterator to the previous cell in the index and returns true to indicate
    /// it was not yet at the beginning of the index. If the iterator is at the first cell the
    /// call does nothing and returns false.
    fn prev(&mut self) -> bool {
        if self.position == 0 {
            return false;
        }

        self.position -= 1;
        self.refresh();
        true
    }

    /// Reports if the iterator is positioned at or after the last index cell.
    fn done(&self) -> bool {
        self.id == CellId::SENTINEL
    }

    /// Updates the stored internal iterator values.
    fn refresh(&mut self) {
        if self.position < self.index.cells.len() {
            self.id = self.index.cells[self.position];
            self.cell = Some(&self.index.cell_map[self.position]);
        } else {
            self.id = CellId::SENTINEL;
            self.cell = None;
        }
    }

    /// Positions the iterator at the first cell whose ID >= target, or at the end of the
    /// index if no such cell exists.
    fn seek(&mut self, target: CellId) {
        self.position = self.index.cells.partition_point(|&c| c < target);
        self.refresh();
    }

    /// Positions the iterator at the cell that contains the given Point. If no such cell
    /// exists, the iterator position is unspecified, and false is returned. The cell at the
    /// matched position is guaranteed to contain all edges that might intersect the line
    /// segment between target and the cell's center.
    pub(crate) fn locate_point(&mut self, p: Point) -> bool {
        // Let I = cellMap.LowerBound(T), where T is the leaf cell containing
        // point P. Then if T is contained by an index cell, then the
        // containing cell is either I or I'. We test for containment by comparing
        // the ranges of leaf cells spanned by T, I, and I'.
        let target = cell_id_from_point(p);
        self.seek(target);
        if !self.done() && self.cell_id().range_min() <= target {
            return true;
        }

        if self.prev() && self.cell_id().range_max() >= target {
            return true;
        }
        false
    }

    /// Attempts to position the iterator at the first matching index cell in the index that
    /// has some relation to the given CellID. Let T be the target CellID. If T is contained by
    /// (or equal to) some index cell I, then the iterator is positioned at I and returns
    /// Indexed. Otherwise if T contains one or more (smaller) index cells, then the iterator
    /// is positioned at the first such cell I and return Subdivided. Otherwise Disjoint is
    /// returned and the iterator position is undefined.
    pub(crate) fn locate_cell_id(&mut self, target: CellId) -> CellRelation {
        // Let T be the target, let I = cellMap.LowerBound(T.RangeMin()), and
        // let I' be the predecessor of I. If T contains any index cells, then T
        // contains I. Similarly, if T is contained by an index cell, then the
        // containing cell is either I or I'. We test for containment by comparing
        // the ranges of leaf cells spanned by T, I, and I'.
        self.seek(target.range_min());
        if !self.done() {
            if self.cell_id() >= target && self.cell_id().range_min() <= target {
                return CellRelation::Indexed;
            }
            if self.cell_id() <= target.range_max() {
                return CellRelation::Subdivided;
            }
        }
        if self.prev() && self.cell_id().range_max() >= target {
            return CellRelation::Indexed;
        }
        CellRelation::Disjoint
    }
}
