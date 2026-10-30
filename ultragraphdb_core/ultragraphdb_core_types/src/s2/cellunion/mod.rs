/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/cellunion.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::s2::cap::Cap;
use crate::s2::cell::Cell;
use crate::s2::cellid::{CellId, MAX_LEVEL};
use crate::s2::metric::AVG_AREA_METRIC;
use crate::s2::point::Point;
use crate::s2::rect::Rect;

/// A collection of CellIDs. It is normalized if it is sorted, and does not contain
/// redundancy. Specifically, it may not contain the same CellID twice, nor a CellID that is
/// contained by another, nor the four sibling CellIDs that are children of a single higher
/// level CellID.
///
/// CellUnions are not required to be normalized, but certain operations will return
/// different results if they are not (e.g. contains_cell_id).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CellUnion(Vec<CellId>);

impl From<Vec<CellId>> for CellUnion {
    fn from(ids: Vec<CellId>) -> Self {
        CellUnion(ids)
    }
}

impl CellUnion {
    /// Wraps the given cell ids without normalizing them, as Go's `s2.CellUnion(ids)`.
    pub fn new(ids: Vec<CellId>) -> CellUnion {
        CellUnion(ids)
    }

    /// Creates a CellUnion that covers the half-open range of leaf cells [begin, end). If
    /// begin == end the resulting union is empty. This requires that begin and end are both
    /// leaves, and begin <= end. To create a closed-ended range, pass in end.Next().
    pub fn from_range(begin: CellId, end: CellId) -> CellUnion {
        let mut cu = Vec::new();
        let mut id = begin.max_tile(end);
        while id != end {
            cu.push(id);
            id = id.next().max_tile(end);
        }
        CellUnion(cu)
    }

    /// The cell ids, in their current order.
    pub fn as_slice(&self) -> &[CellId] {
        &self.0
    }

    /// Consumes the union and returns the cell ids.
    pub fn into_vec(self) -> Vec<CellId> {
        self.0
    }

    /// The number of cell ids.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Reports whether the union holds no cell ids.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterates over the cell ids.
    pub fn iter(&self) -> core::slice::Iter<'_, CellId> {
        self.0.iter()
    }

    /// Appends the cell ids of another union without normalizing, as Go's `append`.
    pub fn extend(&mut self, other: CellUnion) {
        self.0.extend(other.0);
    }

    pub(crate) fn push(&mut self, id: CellId) {
        self.0.push(id);
    }

    pub(crate) fn vec_mut(&mut self) -> &mut Vec<CellId> {
        &mut self.0
    }

    /// Normalizes the CellUnion: sorts the ids, removes duplicates and ids contained by
    /// others, and replaces groups of four child cells by their parent.
    pub fn normalize(&mut self) {
        self.0.sort_unstable();

        let mut output: Vec<CellId> = Vec::with_capacity(self.0.len()); // the list of accepted cells
        // Loop invariant: output is a sorted list of cells with no redundancy.
        for &ci in &self.0 {
            let mut ci = ci;
            // The output is generated in sorted order. Thus the output contains ci
            // if and only if the last cell in output contains ci.

            // If the last cell contains ci, skip it.
            if let Some(last) = output.last()
                && last.contains(ci)
            {
                continue;
            }

            // Discard any previously accepted cells contained by ci.
            while let Some(last) = output.last() {
                if !ci.contains(*last) {
                    break;
                }
                output.pop();
            }

            // See if the last three cells plus this one can be collapsed.
            // We loop because collapsing three accepted cells and adding a
            // higher level cell could cascade into previously accepted cells.
            while output.len() >= 3 {
                let n = output.len();
                if !are_siblings(output[n - 3], output[n - 2], output[n - 1], ci) {
                    break;
                }
                // Replace four cells by their parent cell.
                output.truncate(n - 3);
                ci = ci.immediate_parent(); // checked !ci.isFace above
            }
            output.push(ci);
        }
        self.0 = output;
    }

    /// Reports whether the CellUnion intersects the given cell ID.
    pub fn intersects_cell_id(&self, id: CellId) -> bool {
        // Find index of array item that occurs directly after our probe cell:
        let i = self.0.partition_point(|&c| c <= id);

        if i != self.0.len() && self.0[i].range_min() <= id.range_max() {
            return true;
        }
        i != 0 && self.0[i - 1].range_max() >= id.range_min()
    }

    /// Reports whether the CellUnion contains the given cell ID. Containment is defined with
    /// respect to regions, e.g. a cell contains its 4 children.
    ///
    /// CAVEAT: If you have constructed a non-normalized CellUnion, note that groups of 4 child
    /// cells are *not* considered to contain their parent cell. To get this behavior you must
    /// use one of the call Normalize() explicitly.
    pub fn contains_cell_id(&self, id: CellId) -> bool {
        // Find index of array item that occurs directly after our probe cell:
        let i = self.0.partition_point(|&c| c <= id);

        if i != self.0.len() && self.0[i].range_min() <= id {
            return true;
        }
        i != 0 && self.0[i - 1].range_max() >= id
    }

    /// Replaces this CellUnion with an expanded version of the CellUnion where any cell whose
    /// level is less than minLevel or where (level - minLevel) is not a multiple of levelMod
    /// is replaced by its children, until either both of these conditions are satisfied or
    /// the maximum level is reached. A min level outside 0 to 30 and a level mod outside 1 to 3 are clamped to those
    /// ranges, as the RegionCoverer clamps its own; Go requires them and overflows or loops without end on others.
    pub fn denormalize(&mut self, min_level: i64, level_mod: i64) {
        let min_level = min_level.clamp(0, MAX_LEVEL);
        let level_mod = level_mod.clamp(1, 3);
        let mut denorm: Vec<CellId> = Vec::new();
        for &id in &self.0 {
            let level = id.level();
            let mut new_level = level.max(min_level);
            if level_mod > 1 {
                new_level += (MAX_LEVEL - (new_level - min_level)) % level_mod;
                if new_level > MAX_LEVEL {
                    new_level = MAX_LEVEL;
                }
            }
            if new_level == level {
                denorm.push(id);
            } else {
                let end = id.child_end_at_level(new_level);
                let mut ci = id.child_begin_at_level(new_level);
                while ci != end {
                    denorm.push(ci);
                    ci = ci.next();
                }
            }
        }
        self.0 = denorm;
    }

    /// Returns a Rect that bounds this entity.
    pub fn rect_bound(&self) -> Rect {
        self.0.iter().fold(Rect::empty(), |bound, &c| {
            bound.union(Cell::from_cell_id(c).rect_bound())
        })
    }

    /// Returns a Cap that bounds this entity.
    pub fn cap_bound(&self) -> Cap {
        if self.0.is_empty() {
            return Cap::empty();
        }

        // Compute the approximate centroid of the region. This won't produce the
        // bounding cap of minimal area, but it should be close enough.
        let mut centroid = Point::default();

        for ci in &self.0 {
            let area = AVG_AREA_METRIC.value(ci.level());
            centroid = Point(centroid.0.add(ci.point().0.mul(area)));
        }

        if centroid == Point::default() {
            centroid = Point::from_coords(1.0, 0.0, 0.0);
        } else {
            centroid = Point(centroid.0.normalize());
        }

        // Use the centroid as the cap axis, and expand the cap angle so that it
        // contains the bounding caps of all the individual cells.  Note that it is
        // *not* sufficient to just bound all the cell vertices because the bounding
        // cap may be concave (i.e. cover more than one hemisphere).
        let mut c = Cap::from_point(centroid);
        for &ci in &self.0 {
            c = c.add_cap(Cell::from_cell_id(ci).cap_bound());
        }

        c
    }

    /// Reports whether this cell union contains the given cell.
    pub fn contains_cell(&self, c: &Cell) -> bool {
        self.contains_cell_id(c.id())
    }

    /// Reports whether this cell union intersects the given cell.
    pub fn intersects_cell(&self, c: &Cell) -> bool {
        self.intersects_cell_id(c.id())
    }

    /// Reports whether this cell union contains the given point.
    pub fn contains_point(&self, p: Point) -> bool {
        self.contains_cell(&Cell::from_point(p))
    }

    /// Computes a covering of the CellUnion.
    pub fn cell_union_bound(&self) -> Vec<CellId> {
        self.cap_bound().cell_union_bound()
    }
}

/// Reports whether the given four cells have a common parent.
/// This requires that the four CellIDs are distinct.
fn are_siblings(a: CellId, b: CellId, c: CellId, d: CellId) -> bool {
    // A necessary (but not sufficient) condition is that the XOR of the
    // four cell IDs must be zero. This is also very fast to test.
    if (a.0 ^ b.0 ^ c.0) != d.0 {
        return false;
    }

    // Now we do a slightly more expensive but exact test. First, compute a
    // mask that blocks out the two bits that encode the child position of
    // "id" with respect to its parent, then check that the other three
    // children all agree with "mask".
    let mut mask = d.lsb() << 1;
    mask = !mask.wrapping_add(mask << 1);
    let id_masked = d.0 & mask;
    (a.0 & mask) == id_masked
        && (b.0 & mask) == id_masked
        && (c.0 & mask) == id_masked
        && !d.is_face()
}
