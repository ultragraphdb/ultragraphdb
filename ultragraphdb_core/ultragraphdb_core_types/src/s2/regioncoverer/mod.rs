/*
 * Copyright 2015 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/regioncoverer.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod heap;

use crate::s2::cell::Cell;
use crate::s2::cellid::{CellId, MAX_LEVEL};
use crate::s2::cellunion::CellUnion;
use crate::s2::region::Region;
use crate::s2::util::clamp;

/// Allows arbitrary regions to be approximated as unions of cells (CellUnion). This is useful
/// for implementing various sorts of search and precomputation operations.
///
/// For covering, only cells where (level - MinLevel) is a multiple of LevelMod will be used.
/// This effectively allows the branching factor of the S2 CellID hierarchy to be increased.
/// Currently the only parameter values allowed are 1, 2, or 3, corresponding to branching
/// factors of 4, 16, and 64 respectively.
///
/// Note the following:
///
///  - MinLevel takes priority over MaxCells, i.e. cells below the given level will never be
///    used even if this causes a large number of cells to be returned.
///
///  - For any setting of MaxCells, up to 6 cells may be returned if that is the minimum
///    number of cells required (e.g. if the region intersects all six face cells). Up to 3
///    cells may be returned even for very tiny convex regions if they happen to be located
///    at the intersection of three cube faces.
///
///  - For any setting of MaxCells, an arbitrary number of cells may be returned if MinLevel
///    is too high for the region being approximated.
///
///  - If MaxCells is less than 4, the area of the covering may be arbitrarily large compared
///    to the area of the original region even if the region is convex (e.g. a Cap or Rect).
///
/// The approximation algorithm is not optimal but does a pretty good job in practice. The
/// output does not always use the maximum number of cells allowed, both because this would
/// not always yield a better approximation, and because MaxCells is a limit on how much work
/// is done exploring the possible covering as well as a limit on the final output size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegionCoverer {
    min_level: i64, // the minimum cell level to be used.
    max_level: i64, // the maximum cell level to be used.
    level_mod: i64, // the LevelMod to be used.
    max_cells: i64, // the maximum desired number of cells in the approximation.
}

impl Default for RegionCoverer {
    /// Go's NewRegionCoverer: MinLevel 0, MaxLevel 30, LevelMod 1, MaxCells 8.
    fn default() -> Self {
        RegionCoverer {
            min_level: 0,
            max_level: MAX_LEVEL,
            level_mod: 1,
            max_cells: 8,
        }
    }
}

struct Candidate {
    cell: Cell,
    terminal: bool,           // Cell should not be expanded further.
    num_children: i64,        // Number of children that intersect the region.
    children: Vec<Candidate>, // Actual size may be 0, 4, 16, or 64 elements.
    priority: i64,            // Priority of the candidate.
}

/// We want Pop to give us the highest, not lowest, priority so we use greater than here.
fn higher_priority(a: &Candidate, b: &Candidate) -> bool {
    a.priority > b.priority
}

struct Coverer<'r, R: Region> {
    min_level: i64, // the minimum cell level to be used.
    max_level: i64, // the maximum cell level to be used.
    level_mod: i64, // the LevelMod to be used.
    max_cells: i64, // the maximum desired number of cells in the approximation.
    region: Option<&'r R>,
    result: CellUnion,
    pq: Vec<Candidate>,
}

impl RegionCoverer {
    /// Returns a region coverer with the given settings, as Go's
    /// `RegionCoverer{MinLevel, MaxLevel, LevelMod, MaxCells}` literal.
    pub fn new(min_level: i64, max_level: i64, level_mod: i64, max_cells: i64) -> RegionCoverer {
        RegionCoverer {
            min_level,
            max_level,
            level_mod,
            max_cells,
        }
    }

    /// The minimum cell level to be used.
    pub fn min_level(&self) -> i64 {
        self.min_level
    }

    /// The maximum cell level to be used.
    pub fn max_level(&self) -> i64 {
        self.max_level
    }

    /// The level modulus.
    pub fn level_mod(&self) -> i64 {
        self.level_mod
    }

    /// The maximum desired number of cells in the approximation.
    pub fn max_cells(&self) -> i64 {
        self.max_cells
    }

    /// Returns an instance of coverer.
    fn new_coverer<'r, R: Region>(&self) -> Coverer<'r, R> {
        Coverer {
            min_level: clamp(self.min_level, 0, MAX_LEVEL),
            max_level: clamp(self.max_level, 0, MAX_LEVEL),
            level_mod: clamp(self.level_mod, 1, 3),
            max_cells: self.max_cells,
            region: None,
            result: CellUnion::default(),
            pq: Vec::new(),
        }
    }

    /// Returns a CellUnion that covers the given region and satisfies the various
    /// restrictions.
    pub fn covering<R: Region>(&self, region: &R) -> CellUnion {
        let mut covering = self.cell_union(region);
        covering.denormalize(
            clamp(self.min_level, 0, MAX_LEVEL),
            clamp(self.level_mod, 1, 3),
        );
        covering
    }

    /// Returns a normalized CellUnion that covers the given region and satisfies the
    /// restrictions except for minLevel and levelMod. These criteria cannot be satisfied
    /// using a cell union because cell unions are automatically normalized by replacing four
    /// child cells with their parent whenever possible. (Note that the list of cell ids passed
    /// to the CellUnion constructor does in fact satisfy all the given restrictions.)
    pub fn cell_union<R: Region>(&self, region: &R) -> CellUnion {
        let mut c = self.new_coverer::<R>();
        c.covering_internal(region);
        let mut cu = core::mem::take(&mut c.result);
        cu.normalize();
        cu
    }

    /// Returns a CellUnion that covers the given region similar to Covering, except that this
    /// method is much faster and the coverings are not as tight. All of the usual parameters
    /// are respected (MaxCells, MinLevel, MaxLevel, and LevelMod), except that the
    /// implementation makes no attempt to take advantage of large values of MaxCells. (A
    /// small number of cells will always be returned.)
    ///
    /// This function is useful as a starting point for algorithms that recursively subdivide
    /// cells.
    pub fn fast_covering<R: Region>(&self, region: &R) -> CellUnion {
        let c = self.new_coverer::<R>();
        let mut cu = CellUnion::new(region.cell_union_bound());
        c.normalize_covering(&mut cu);
        cu
    }
}

impl<'r, R: Region> Coverer<'r, R> {
    fn region(&self) -> &'r R {
        self.region.expect("coverer region is set while covering")
    }

    /// Returns a new candidate with no children if the cell intersects the given region. The
    /// candidate is marked as terminal if it should not be expanded further.
    fn new_candidate(&self, cell: Cell) -> Option<Candidate> {
        if !self.region().intersects_cell(&cell) {
            return None;
        }
        let mut cand = Candidate {
            cell,
            terminal: false,
            num_children: 0,
            children: Vec::new(),
            priority: 0,
        };
        let level = cell.level();
        if level >= self.min_level
            && (level + self.level_mod > self.max_level || self.region().contains_cell(&cell))
        {
            cand.terminal = true;
        }
        Some(cand)
    }

    /// Populates the children of the candidate by expanding the given number of levels from
    /// the given cell. Returns the number of children that were marked "terminal".
    fn expand_children(&self, cand: &mut Candidate, cell: Cell, num_levels: i64) -> i64 {
        let num_levels = num_levels - 1;
        let mut num_terminals = 0;
        let last = cell.id().child_end();
        let mut ci = cell.id().child_begin();
        while ci != last {
            let child_cell = Cell::from_cell_id(ci);
            if num_levels > 0 {
                if self.region().intersects_cell(&child_cell) {
                    num_terminals += self.expand_children(cand, child_cell, num_levels);
                }
                ci = ci.next();
                continue;
            }
            if let Some(child) = self.new_candidate(child_cell) {
                if child.terminal {
                    num_terminals += 1;
                }
                cand.children.push(child);
                cand.num_children += 1;
            }
            ci = ci.next();
        }
        num_terminals
    }

    /// Adds the given candidate to the result if it is marked as "terminal", otherwise
    /// expands its children and inserts it into the priority queue. Passing None does
    /// nothing.
    fn add_candidate(&mut self, cand: Option<Candidate>) {
        let Some(mut cand) = cand else {
            return;
        };

        if cand.terminal {
            self.result.push(cand.cell.id());
            return;
        }

        // Expand one level at a time until we hit minLevel to ensure that we don't skip over it.
        let level = cand.cell.level();
        let num_levels = if level < self.min_level {
            1
        } else {
            self.level_mod
        };

        let cell = cand.cell;
        let num_terminals = self.expand_children(&mut cand, cell, num_levels);
        let max_children_shift = 2 * self.level_mod;
        if cand.num_children == 0 {
            // Not added to the queue.
        } else if num_terminals == 1 << max_children_shift && level >= self.min_level {
            // Optimization: add the parent cell rather than all of its children.
            // We can't do this for interior coverings, since the children just
            // intersect the region, but may not be contained by it - we need to
            // subdivide them further.
            cand.terminal = true;
            self.add_candidate(Some(cand));
        } else {
            // We negate the priority so that smaller absolute priorities are returned
            // first. The heuristic is designed to refine the largest cells first,
            // since those are where we have the largest potential gain. Among cells
            // of the same size, we prefer the cells with the fewest children.
            // Finally, among cells with equal numbers of children we prefer those
            // with the smallest number of children that cannot be refined further.
            cand.priority = -((((level << max_children_shift) + cand.num_children)
                << max_children_shift)
                + num_terminals);
            heap::push(&mut self.pq, cand, higher_priority);
        }
    }

    /// Returns the reduced "level" so that it satisfies levelMod. Levels smaller than
    /// minLevel are not affected (since cells at these levels are eventually expanded).
    fn adjust_level(&self, level: i64) -> i64 {
        if self.level_mod > 1 && level > self.min_level {
            return level - (level - self.min_level) % self.level_mod;
        }
        level
    }

    /// Ensures that all cells with level > minLevel also satisfy levelMod, by replacing them
    /// with an ancestor if necessary. Cell levels smaller than minLevel are not modified (see
    /// AdjustLevel). The output is then normalized to ensure that no redundant cells are
    /// present.
    fn adjust_cell_levels(&self, cells: &mut CellUnion) {
        if self.level_mod == 1 {
            return;
        }

        let v = cells.vec_mut();
        let mut out = 0;
        for k in 0..v.len() {
            let mut ci = v[k];
            let level = ci.level();
            let new_level = self.adjust_level(level);
            if new_level != level {
                ci = ci.parent(new_level);
            }
            if out > 0 && v[out - 1].contains(ci) {
                continue;
            }
            while out > 0 && ci.contains(v[out - 1]) {
                out -= 1;
            }
            v[out] = ci;
            out += 1;
        }
        v.truncate(out);
    }

    /// Computes a set of initial candidates that cover the given region.
    fn initial_candidates(&mut self) {
        // Optimization: start with a small (usually 4 cell) covering of the region's bounding cap.
        let temp = RegionCoverer {
            min_level: 0,
            max_level: self.max_level,
            level_mod: 1,
            max_cells: self.max_cells.min(4),
        };

        let mut cells = temp.fast_covering(self.region());
        self.adjust_cell_levels(&mut cells);
        for &ci in cells.iter() {
            let cand = self.new_candidate(Cell::from_cell_id(ci));
            self.add_candidate(cand);
        }
    }

    /// Generates a covering and stores it in result. Strategy: Start with the 6 faces of the
    /// cube. Discard any that do not intersect the shape. Then repeatedly choose the largest
    /// cell that intersects the shape and subdivide it.
    ///
    /// result contains the cells that will be part of the output, while pq contains cells
    /// that we may still subdivide further. Cells that are entirely contained within the
    /// region are immediately added to the output, while cells that do not intersect the
    /// region are immediately discarded. Therefore pq only contains cells that partially
    /// intersect the region. Candidates are prioritized first according to cell size (larger
    /// cells first), then by the number of intersecting children they have (fewest children
    /// first), and then by the number of fully contained children (fewest children first).
    fn covering_internal(&mut self, region: &'r R) {
        self.region = Some(region);

        self.initial_candidates();
        while let Some(mut cand) = heap::pop(&mut self.pq, higher_priority) {
            // For exterior covering we cannot do this, because result has to cover the
            // whole region, so all children have to be used.
            // candidate.numChildren == 1 case takes care of the situation when we
            // already have more than MaxCells in result (minLevel is too high).
            // Subdividing of the candidate with one child does no harm in this case.
            if cand.cell.level() < self.min_level
                || cand.num_children == 1
                || self.result.len() as i64 + self.pq.len() as i64 + cand.num_children
                    <= self.max_cells
            {
                for child in core::mem::take(&mut cand.children) {
                    self.add_candidate(Some(child));
                }
            } else {
                cand.terminal = true;
                self.add_candidate(Some(cand));
            }
        }

        self.pq.clear();
        self.region = None;

        // Rather than just returning the raw list of cell ids, we construct a cell
        // union and then denormalize it. This has the effect of replacing four
        // child cells with their parent whenever this does not violate the covering
        // parameters specified (min_level, level_mod, etc). This significantly
        // reduces the number of cells returned in many cases, and it is cheap
        // compared to computing the covering in the first place.
        self.result.normalize();
        if self.min_level > 0 || self.level_mod > 1 {
            self.result.denormalize(self.min_level, self.level_mod);
        }
    }

    /// Normalizes the "covering" so that it conforms to the current covering parameters
    /// (maxCells, minLevel, MaxLevel, and levelMod). This method makes no attempt to be
    /// optimal. In particular, if minLevel > 0 or levelMod > 1 then it may return more than
    /// the desired number of cells even when this isn't necessary.
    ///
    /// Note that when the covering parameters have their default values, almost all of the
    /// code in this function is skipped.
    fn normalize_covering(&self, covering: &mut CellUnion) {
        // If any cells are too small, or don't satisfy levelMod, then replace them with ancestors.
        if self.max_level < MAX_LEVEL || self.level_mod > 1 {
            for ci in covering.vec_mut().iter_mut() {
                let level = ci.level();
                let new_level = self.adjust_level(level.min(self.max_level));
                if new_level != level {
                    *ci = ci.parent(new_level);
                }
            }
        }
        // Sort the cells and simplify them.
        covering.normalize();

        // Make sure that the covering satisfies minLevel and levelMod,
        // possibly at the expense of satisfying MaxCells.
        if self.min_level > 0 || self.level_mod > 1 {
            covering.denormalize(self.min_level, self.level_mod);
        }

        // If there are too many cells and the covering is very large, use the
        // RegionCoverer to compute a new covering. (This avoids possible O(n^2)
        // behavior of the simpler algorithm below.)
        // Saturating where Go's int64 arithmetic wraps on a max cells near its limits.
        let excess = (covering.len() as i64).saturating_sub(self.max_cells);
        if excess <= 0 || self.is_canonical(covering) {
            return;
        }
        if excess.saturating_mul(covering.len() as i64) > 10000 {
            let rc = RegionCoverer::default();
            *covering = rc.covering(&covering.clone());
            return;
        }

        // If there are still too many cells, then repeatedly replace two adjacent
        // cells in CellID order by their lowest common ancestor.
        while covering.len() as i64 > self.max_cells {
            let mut best_index: i64 = -1;
            let mut best_level: i64 = -1;
            let v = covering.as_slice();
            for i in 0..v.len().saturating_sub(1) {
                let Some(level) = v[i].common_ancestor_level(v[i + 1]) else {
                    continue;
                };
                let level = self.adjust_level(level);
                if level > best_level {
                    best_level = level;
                    best_index = i as i64;
                }
            }

            if best_level < self.min_level {
                break;
            }

            // Replace all cells contained by the new ancestor cell.
            let mut id = covering.as_slice()[best_index as usize].parent(best_level);
            replace_cells_with_ancestor(covering, id);

            // Now repeatedly check whether all children of the parent cell are
            // present, in which case we can replace those cells with their parent.
            while best_level > self.min_level {
                best_level -= self.level_mod;
                id = id.parent(best_level);
                if !self.contains_all_children(covering.as_slice(), id) {
                    break;
                }
                replace_cells_with_ancestor(covering, id);
            }
        }
    }

    /// Reports whether the covering is canonical.
    fn is_canonical(&self, covering: &CellUnion) -> bool {
        let true_max = if self.level_mod != 1 {
            self.max_level - (self.max_level - self.min_level) % self.level_mod
        } else {
            self.max_level
        };
        let too_many_cells = covering.len() as i64 > self.max_cells;
        let mut same_parent_count = 1;

        let mut prev_id = CellId(0);
        for &id in covering.iter() {
            if !id.is_valid() {
                return false;
            }

            // Check that the CellID level is acceptable.
            let level = id.level();
            if level < self.min_level || level > true_max {
                return false;
            }
            if self.level_mod > 1 && (level - self.min_level) % self.level_mod != 0 {
                return false;
            }

            if prev_id != CellId(0) {
                // Check that cells are sorted and non-overlapping.
                if prev_id.range_max() >= id.range_min() {
                    return false;
                }

                // If there are too many cells, check that no pair of adjacent cells
                // could be replaced by an ancestor.
                if too_many_cells
                    && let Some(lev) = id.common_ancestor_level(prev_id)
                    && lev >= self.min_level
                {
                    return false;
                }

                // Check that there are no sequences of (4 ** level_mod) cells that all
                // have the same parent (considering only multiples of "level_mod").
                let p_level = level - self.level_mod;
                if p_level < self.min_level
                    || level != prev_id.level()
                    || id.parent(p_level) != prev_id.parent(p_level)
                {
                    same_parent_count = 1;
                } else {
                    same_parent_count += 1;
                    if same_parent_count == 1 << (2 * self.level_mod) {
                        return false;
                    }
                }
            }
            prev_id = id;
        }

        true
    }

    fn contains_all_children(&self, covering: &[CellId], id: CellId) -> bool {
        let mut pos = covering.partition_point(|&c| c < id.range_min());
        let level = id.level() + self.level_mod;
        let end = id.child_end_at_level(level);
        let mut child = id.child_begin_at_level(level);
        while child != end {
            if pos == covering.len() || covering[pos] != child {
                return false;
            }
            pos += 1;
            child = child.next();
        }
        true
    }
}

/// Replaces all descendants of the given id in covering with id. This requires the covering
/// contains at least one descendant of id.
fn replace_cells_with_ancestor(covering: &mut CellUnion, id: CellId) {
    let v = covering.vec_mut();
    let begin = v.partition_point(|&c| c <= id.range_min());
    let end = v.partition_point(|&c| c <= id.range_max());
    v.splice(begin..end, [id]);
}
