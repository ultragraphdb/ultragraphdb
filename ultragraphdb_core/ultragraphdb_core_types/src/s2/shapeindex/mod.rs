/*
 * Copyright 2016 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/shapeindex.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The part of golang/geo's ShapeIndex a Loop uses: an index of one shape, built in a single
//! batch the way Go applies its first update (`applyUpdatesInternal` with no prior cells, so
//! every cell is disjoint from the index). Removal, incremental updates and absorbing
//! existing index cells are never reached by a Loop and are left out.

mod iterator;
mod tracker;

pub(crate) use iterator::{CellRelation, ShapeIndexIterator};
use tracker::Tracker;

use crate::s2::cellid::{CellId, NUM_FACES};
use crate::s2::cellunion::CellUnion;
use crate::s2::edge_clipping::{clip_to_padded_face, interpolate_float64};
use crate::s2::metric::AVG_EDGE_METRIC;
use crate::s2::paddedcell::PaddedCell;
use crate::s2::r1::R1Interval;
use crate::s2::r2::{R2Point, R2Rect};
use crate::s2::shape::{Edge, Shape};
use crate::s2::shapeutil::contains_brute_force;
use crate::s2::stuv::{face, valid_face_xyz_to_uv};

/// The total error when clipping an edge which comes from two sources: (1) Clipping the
/// original spherical edge to a cube face (the face edge). The maximum error in this step is
/// faceClipErrorUVCoord. (2) Clipping the face edge to the u- or v-coordinate of a cell
/// boundary. The maximum error in this step is edgeClipErrorUVCoord. Finally, since we
/// encounter the same errors when clipping query edges, we double the total error so that we
/// only need to pad edges during indexing and not at query time. Go's constant
/// `2.0 * (faceClipErrorUVCoord + edgeClipErrorUVCoord)`.
pub(crate) const CELL_PADDING: f64 = f64::from_bits(0x3cf13a5919a791a3);

/// Go's constant `1 - cellPadding`, rounded once from the exact difference.
const MAX_UV: f64 = f64::from_bits(0x3fefffffffffffde);

/// The cell size relative to the length of an edge at which it is first considered to be
/// long. Long edges do not contribute toward the decision to subdivide a cell further.
const CELL_SIZE_TO_LONG_EDGE_RATIO: f64 = 1.0;

/// The maximum number of edges per cell (Go's NewShapeIndex default).
const MAX_EDGES_PER_CELL: usize = 10;

/// The part of a shape that intersects a Cell. It consists of the set of edge IDs that
/// intersect that cell and a boolean indicating whether the center of the cell is inside the
/// shape (for shapes that have an interior).
///
/// Note that the edges themselves are not clipped; we always use the original edges for
/// intersection tests so that the results will be the same as the original shape.
#[derive(Clone, Debug)]
pub(crate) struct ClippedShape {
    shape_id: i32,
    pub(crate) contains_center: bool,
    pub(crate) edges: Vec<usize>,
}

/// The index contents for a particular CellID.
#[derive(Clone, Debug)]
pub(crate) struct ShapeIndexCell {
    shapes: Vec<ClippedShape>,
}

impl ShapeIndexCell {
    /// Returns the clipped shape that contains the given shapeID, or None if none of the
    /// clipped shapes contain it.
    pub(crate) fn find_by_shape_id(&self, shape_id: i32) -> Option<&ClippedShape> {
        // Linear search is fine because the number of shapes per cell is typically
        // very small (most often 1), and is large only for pathological inputs
        // (e.g. very deeply nested loops).
        self.shapes.iter().find(|c| c.shape_id == shape_id)
    }
}

/// An edge that has been projected onto a given face.
#[derive(Clone, Copy, Debug)]
struct FaceEdge {
    shape_id: i32,      // The ID of shape that this edge belongs to
    edge_id: usize,     // Edge ID within that shape
    max_level: i64,     // Not desirable to subdivide this edge beyond this level
    has_interior: bool, // Belongs to a shape that has a dimension of 2
    a: R2Point,         // The edge endpoints, clipped to a given face
    b: R2Point,
    edge: Edge, // The original edge.
}

/// The portion of a face edge that has been clipped to a given Cell.
#[derive(Clone, Copy, Debug)]
struct ClippedEdge {
    face_edge: usize, // The original unclipped edge, an index into the face's edges
    bound: R2Rect,    // Bounding box for the clipped portion
}

/// Indexes the edges of one shape. The index cells, sorted by CellID, each list the edges
/// that intersect them and whether the shape contains the cell center.
#[derive(Clone, Debug, Default)]
pub(crate) struct ShapeIndex {
    cells: Vec<CellId>,
    cell_map: Vec<ShapeIndexCell>,
}

impl ShapeIndex {
    /// The index of no shapes.
    pub(crate) fn empty() -> ShapeIndex {
        ShapeIndex::default()
    }

    /// Builds the index of the given shape as shape 0, as Go's first `applyUpdatesInternal`.
    pub(crate) fn build<S: Shape>(shape: &S) -> ShapeIndex {
        let mut s = ShapeIndex::default();
        let mut t = Tracker::new();

        // allEdges maps a Face to a collection of faceEdges.
        let mut all_edges: [Vec<FaceEdge>; NUM_FACES] = Default::default();

        s.add_shape_internal(0, shape, &mut all_edges, &mut t);

        for (face, edges) in all_edges.iter().enumerate() {
            s.update_face_edges(face, edges, &mut t);
        }
        s
    }

    /// Returns an iterator positioned at the beginning of the index.
    pub(crate) fn iterator(&self) -> ShapeIndexIterator<'_> {
        ShapeIndexIterator::new(self)
    }

    /// The number of shapes in the index, as Go's `Len` reads it while building (one).
    fn len(&self) -> i32 {
        1
    }

    /// Clips all edges of the given shape to the six cube faces, adds the clipped edges to
    /// the set of allEdges, and starts tracking its interior if necessary.
    fn add_shape_internal<S: Shape>(
        &mut self,
        shape_id: i32,
        shape: &S,
        all_edges: &mut [Vec<FaceEdge>; NUM_FACES],
        t: &mut Tracker,
    ) {
        let has_interior = shape.dimension() == 2;
        if has_interior {
            t.add_shape(shape_id, contains_brute_force(shape, t.focus()));
        }

        for e in 0..shape.num_edges() {
            let edge = shape.edge(e);
            let fe = FaceEdge {
                shape_id,
                edge_id: e,
                max_level: max_level_for_edge(edge),
                has_interior,
                a: R2Point::default(),
                b: R2Point::default(),
                edge,
            };
            add_face_edge(fe, all_edges);
        }
    }

    /// Adds or removes the various edges from the index.
    fn update_face_edges(&mut self, face: usize, face_edges: &[FaceEdge], t: &mut Tracker) {
        let num_edges = face_edges.len();
        if num_edges == 0 && t.shape_ids.is_empty() {
            return;
        }

        // Create the initial clippedEdge for each faceEdge. Additional clipped
        // edges are created when edges are split between child cells.
        let mut clipped_edges: Vec<ClippedEdge> = Vec::with_capacity(num_edges);
        let mut bound = R2Rect::empty();
        for (e, fe) in face_edges.iter().enumerate() {
            let clipped = ClippedEdge {
                face_edge: e,
                bound: R2Rect::from_points(&[fe.a, fe.b]),
            };
            bound = bound.add_rect(clipped.bound);
            clipped_edges.push(clipped);
        }

        // Construct the initial face cell containing all the edges, and then update
        // all the edges in the index recursively.
        let face_id = CellId::from_face(face);
        let mut pcell = PaddedCell::from_cell_id(face_id, CELL_PADDING);

        if num_edges > 0 {
            let shrunk_id = pcell.shrink_to_fit(bound);
            if shrunk_id != pcell.id {
                // All the edges are contained by some descendant of the face cell. We
                // can save a lot of work by starting directly with that cell, but if we
                // are in the interior of at least one shape then we need to create
                // index entries for the cells we are skipping over.
                self.skip_cell_range(face_id.range_min(), shrunk_id.range_min(), face_edges, t);
                pcell = PaddedCell::from_cell_id(shrunk_id, CELL_PADDING);
                self.update_edges(&pcell, clipped_edges, face_edges, t);
                self.skip_cell_range(
                    shrunk_id.range_max().next(),
                    face_id.range_max().next(),
                    face_edges,
                    t,
                );
                return;
            }
        }

        // Otherwise (no edges, or no shrinking is possible), subdivide normally.
        self.update_edges(&pcell, clipped_edges, face_edges, t);
    }

    /// Skips over the cells in the given range, creating index cells if we are currently in
    /// the interior of at least one shape.
    fn skip_cell_range(
        &mut self,
        begin: CellId,
        end: CellId,
        face_edges: &[FaceEdge],
        t: &mut Tracker,
    ) {
        // If we aren't in the interior of a shape, then skipping over cells is easy.
        if t.shape_ids.is_empty() {
            return;
        }

        // Otherwise generate the list of cell ids that we need to visit, and create
        // an index entry for each one.
        let skipped = CellUnion::from_range(begin, end);
        for &cell in skipped.iter() {
            self.update_edges(
                &PaddedCell::from_cell_id(cell, CELL_PADDING),
                Vec::new(),
                face_edges,
                t,
            );
        }
    }

    /// Adds the given edges whose bounding boxes intersect a given cell. The index holds no
    /// cell overlapping the given cell (Go's disjointFromIndex is true on a first update).
    fn update_edges(
        &mut self,
        pcell: &PaddedCell,
        edges: Vec<ClippedEdge>,
        face_edges: &[FaceEdge],
        t: &mut Tracker,
    ) {
        // This function is recursive with a maximum recursion depth of 30 (MaxLevel).

        // makeIndexCell checks if the number of edges is small enough, and creates
        // an index cell if possible (returning true when it does so).
        if self.make_index_cell(pcell, &edges, face_edges, t) {
            return;
        }

        let mut child_edges: [[Vec<ClippedEdge>; 2]; 2] = Default::default(); // [i][j]

        // Compute the middle of the padded cell, defined as the rectangle in
        // (u,v)-space that belongs to all four (padded) children. By comparing
        // against the four boundaries of middle we can determine which children
        // each edge needs to be propagated to.
        let middle = pcell.middle();

        // Build up a vector edges to be passed to each child cell. The (i,j)
        // directions are left (i=0), right (i=1), lower (j=0), and upper (j=1).
        // Note that the vast majority of edges are propagated to a single child.
        for edge in &edges {
            if edge.bound.x.hi <= middle.x.lo {
                // Edge is entirely contained in the two left children.
                let (a, b) = clip_v_axis(*edge, middle.y, face_edges);
                if let Some(a) = a {
                    child_edges[0][0].push(a);
                }
                if let Some(b) = b {
                    child_edges[0][1].push(b);
                }
            } else if edge.bound.x.lo >= middle.x.hi {
                // Edge is entirely contained in the two right children.
                let (a, b) = clip_v_axis(*edge, middle.y, face_edges);
                if let Some(a) = a {
                    child_edges[1][0].push(a);
                }
                if let Some(b) = b {
                    child_edges[1][1].push(b);
                }
            } else if edge.bound.y.hi <= middle.y.lo {
                // Edge is entirely contained in the two lower children.
                child_edges[0][0].push(clip_u_bound(*edge, 1, middle.x.hi, face_edges));
                child_edges[1][0].push(clip_u_bound(*edge, 0, middle.x.lo, face_edges));
            } else if edge.bound.y.lo >= middle.y.hi {
                // Edge is entirely contained in the two upper children.
                child_edges[0][1].push(clip_u_bound(*edge, 1, middle.x.hi, face_edges));
                child_edges[1][1].push(clip_u_bound(*edge, 0, middle.x.lo, face_edges));
            } else {
                // The edge bound spans all four children. The edge
                // itself intersects either three or four padded children.
                let left = clip_u_bound(*edge, 1, middle.x.hi, face_edges);
                let (a, b) = clip_v_axis(left, middle.y, face_edges);
                if let Some(a) = a {
                    child_edges[0][0].push(a);
                }
                if let Some(b) = b {
                    child_edges[0][1].push(b);
                }
                let right = clip_u_bound(*edge, 0, middle.x.lo, face_edges);
                let (a, b) = clip_v_axis(right, middle.y, face_edges);
                if let Some(a) = a {
                    child_edges[1][0].push(a);
                }
                if let Some(b) = b {
                    child_edges[1][1].push(b);
                }
            }
        }

        // Now recursively update the edges in each child. We call the children in
        // increasing order of CellID so that when the index is first constructed,
        // all insertions into cellMap are at the end (which is much faster).
        for pos in 0..4 {
            let (i, j) = pcell.child_ij(pos);
            if !child_edges[i][j].is_empty() || !t.shape_ids.is_empty() {
                let child = PaddedCell::from_parent_ij(pcell, i, j);
                let ce = core::mem::take(&mut child_edges[i][j]);
                self.update_edges(&child, ce, face_edges, t);
            }
        }
    }

    /// Builds an indexCell from the given padded cell and set of edges and adds it to the
    /// index. If the cell or edges are empty, no cell is added.
    fn make_index_cell(
        &mut self,
        p: &PaddedCell,
        edges: &[ClippedEdge],
        face_edges: &[FaceEdge],
        t: &mut Tracker,
    ) -> bool {
        // If the cell is empty, no index cell is needed. (In most cases this
        // situation is detected before we get to this point, but this can happen
        // when all shapes in a cell are removed.)
        if edges.is_empty() && t.shape_ids.is_empty() {
            return true;
        }

        // Count the number of edges that have not reached their maximum level yet.
        // Return false if there are too many such edges.
        let mut count = 0;
        for ce in edges {
            if p.level() < face_edges[ce.face_edge].max_level {
                count += 1;
            }

            if count > MAX_EDGES_PER_CELL {
                return false;
            }
        }

        // We update the InteriorTracker as follows. For every Cell in the index
        // we construct two edges: one edge from entry vertex of the cell to its
        // center, and one from the cell center to its exit vertex. Here entry
        // and exit refer the CellID ordering, i.e. the order in which points
        // are encountered along the 2 space-filling curve. The exit vertex then
        // becomes the entry vertex for the next cell in the index, unless there are
        // one or more empty intervening cells, in which case the InteriorTracker
        // state is unchanged because the intervening cells have no edges.

        // Shift the InteriorTracker focus point to the center of the current cell.
        if t.is_active && !edges.is_empty() {
            if !t.at_cell_id(p.id) {
                t.move_to(p.entry_vertex());
            }
            t.draw_to(p.center());
            test_all_edges(edges, face_edges, t);
        }

        // Allocate and fill a new index cell. To get the total number of shapes we
        // need to merge the shapes associated with the intersecting edges together
        // with the shapes that happen to contain the cell center.
        let cshape_ids = t.shape_ids.clone();
        let num_shapes = count_shapes(edges, face_edges, &cshape_ids);
        let mut cell = ShapeIndexCell {
            shapes: Vec::with_capacity(num_shapes),
        };

        // To fill the index cell we merge the two sources of shapes: edge shapes
        // (those that have at least one edge that intersects this cell), and
        // containing shapes (those that contain the cell center). We keep track
        // of the index of the next intersecting edge and the next containing shape
        // as we go along. Both sets of shape ids are already sorted.
        let mut e_next = 0;
        let mut c_next_idx = 0;
        for _ in 0..num_shapes {
            // advance to next value base + i
            let mut eshape_id = self.len();
            let mut cshape_id = eshape_id; // Sentinels

            if e_next != edges.len() {
                eshape_id = face_edges[edges[e_next].face_edge].shape_id;
            }
            if c_next_idx < cshape_ids.len() {
                cshape_id = cshape_ids[c_next_idx];
            }
            let e_begin = e_next;
            let clipped = if cshape_id < eshape_id {
                // The entire cell is in the shape interior.
                c_next_idx += 1;
                ClippedShape {
                    shape_id: cshape_id,
                    contains_center: true,
                    edges: Vec::new(),
                }
            } else {
                // Count the number of edges for this shape and allocate space for them.
                while e_next < edges.len()
                    && face_edges[edges[e_next].face_edge].shape_id == eshape_id
                {
                    e_next += 1;
                }
                let ids = edges[e_begin..e_next]
                    .iter()
                    .map(|e| face_edges[e.face_edge].edge_id)
                    .collect();
                let mut clipped = ClippedShape {
                    shape_id: eshape_id,
                    contains_center: false,
                    edges: ids,
                };
                if cshape_id == eshape_id {
                    clipped.contains_center = true;
                    c_next_idx += 1;
                }
                clipped
            };
            cell.shapes.push(clipped);
        }

        // Add this cell to the map.
        self.cells.push(p.id);
        self.cell_map.push(cell);

        // Shift the tracker focus point to the exit vertex of this cell.
        if t.is_active && !edges.is_empty() {
            t.draw_to(p.exit_vertex());
            test_all_edges(edges, face_edges, t);
            t.set_next_cell_id(p.id.next());
        }
        true
    }
}

/// Adds the given faceEdge into the collection of all edges.
fn add_face_edge(fe: FaceEdge, all_edges: &mut [Vec<FaceEdge>; NUM_FACES]) {
    let mut fe = fe;
    let a_face = face(fe.edge.v0.0);
    // See if both endpoints are on the same face, and are far enough from
    // the edge of the face that they don't intersect any (padded) adjacent face.
    if a_face == face(fe.edge.v1.0) {
        let (x, y) = valid_face_xyz_to_uv(a_face, fe.edge.v0.0);
        fe.a = R2Point::new(x, y);
        let (x, y) = valid_face_xyz_to_uv(a_face, fe.edge.v1.0);
        fe.b = R2Point::new(x, y);

        if fe.a.x.abs() <= MAX_UV
            && fe.a.y.abs() <= MAX_UV
            && fe.b.x.abs() <= MAX_UV
            && fe.b.y.abs() <= MAX_UV
        {
            all_edges[a_face].push(fe);
            return;
        }
    }

    // Otherwise, we simply clip the edge to all six faces.
    for (face, edges) in all_edges.iter_mut().enumerate() {
        if let Some((a_clip, b_clip)) =
            clip_to_padded_face(fe.edge.v0, fe.edge.v1, face, CELL_PADDING)
        {
            fe.a = a_clip;
            fe.b = b_clip;
            edges.push(fe);
        }
    }
}

/// Updates the specified endpoint of the given clipped edge and returns the resulting
/// clipped edge.
fn update_bound(edge: ClippedEdge, u_end: usize, u: f64, v_end: usize, v: f64) -> ClippedEdge {
    let mut c = ClippedEdge {
        face_edge: edge.face_edge,
        bound: R2Rect::default(),
    };
    if u_end == 0 {
        c.bound.x.lo = u;
        c.bound.x.hi = edge.bound.x.hi;
    } else {
        c.bound.x.lo = edge.bound.x.lo;
        c.bound.x.hi = u;
    }

    if v_end == 0 {
        c.bound.y.lo = v;
        c.bound.y.hi = edge.bound.y.hi;
    } else {
        c.bound.y.lo = edge.bound.y.lo;
        c.bound.y.hi = v;
    }

    c
}

/// Clips the given endpoint (lo=0, hi=1) of the u-axis so that it does not extend past the
/// given value of the given edge.
fn clip_u_bound(edge: ClippedEdge, u_end: usize, u: f64, face_edges: &[FaceEdge]) -> ClippedEdge {
    // First check whether the edge actually requires any clipping. (Sometimes
    // this method is called when clipping is not necessary, e.g. when one edge
    // endpoint is in the overlap area between two padded child cells.)
    if u_end == 0 {
        if edge.bound.x.lo >= u {
            return edge;
        }
    } else if edge.bound.x.hi <= u {
        return edge;
    }
    // We interpolate the new v-value from the endpoints of the original edge.
    // This has two advantages: (1) we don't need to store the clipped endpoints
    // at all, just their bounding box; and (2) it avoids the accumulation of
    // roundoff errors due to repeated interpolations. The result needs to be
    // clamped to ensure that it is in the appropriate range.
    let e = &face_edges[edge.face_edge];
    let v = edge
        .bound
        .y
        .clamp_point(interpolate_float64(u, e.a.x, e.b.x, e.a.y, e.b.y));

    // Determine which endpoint of the v-axis bound to update. If the edge
    // slope is positive we update the same endpoint, otherwise we update the
    // opposite endpoint.
    let positive_slope = (e.a.x > e.b.x) == (e.a.y > e.b.y);
    let v_end = if (u_end == 1) == positive_slope { 1 } else { 0 };
    update_bound(edge, u_end, u, v_end, v)
}

/// Clips the given endpoint (lo=0, hi=1) of the v-axis so that it does not extend past the
/// given value of the given edge.
fn clip_v_bound(edge: ClippedEdge, v_end: usize, v: f64, face_edges: &[FaceEdge]) -> ClippedEdge {
    if v_end == 0 {
        if edge.bound.y.lo >= v {
            return edge;
        }
    } else if edge.bound.y.hi <= v {
        return edge;
    }

    // We interpolate the new v-value from the endpoints of the original edge.
    // This has two advantages: (1) we don't need to store the clipped endpoints
    // at all, just their bounding box; and (2) it avoids the accumulation of
    // roundoff errors due to repeated interpolations. The result needs to be
    // clamped to ensure that it is in the appropriate range.
    let e = &face_edges[edge.face_edge];
    let u = edge
        .bound
        .x
        .clamp_point(interpolate_float64(v, e.a.y, e.b.y, e.a.x, e.b.x));

    // Determine which endpoint of the v-axis bound to update. If the edge
    // slope is positive we update the same endpoint, otherwise we update the
    // opposite endpoint.
    let positive_slope = (e.a.x > e.b.x) == (e.a.y > e.b.y);
    let u_end = if (v_end == 1) == positive_slope { 1 } else { 0 };
    update_bound(edge, u_end, u, v_end, v)
}

/// Returns the given edge clipped to within the boundaries of the middle interval along the
/// v-axis, and adds the result to its children.
fn clip_v_axis(
    edge: ClippedEdge,
    middle: R1Interval,
    face_edges: &[FaceEdge],
) -> (Option<ClippedEdge>, Option<ClippedEdge>) {
    if edge.bound.y.hi <= middle.lo {
        // Edge is entirely contained in the lower child.
        return (Some(edge), None);
    } else if edge.bound.y.lo >= middle.hi {
        // Edge is entirely contained in the upper child.
        return (None, Some(edge));
    }
    // The edge bound spans both children.
    (
        Some(clip_v_bound(edge, 1, middle.hi, face_edges)),
        Some(clip_v_bound(edge, 0, middle.lo, face_edges)),
    )
}

/// Calls the trackers testEdge on all edges from shapes that have interiors.
fn test_all_edges(edges: &[ClippedEdge], face_edges: &[FaceEdge], t: &mut Tracker) {
    for edge in edges {
        let fe = &face_edges[edge.face_edge];
        if fe.has_interior {
            t.test_edge(fe.shape_id, fe.edge);
        }
    }
}

/// Reports the number of distinct shapes that are either associated with the given edges,
/// or that are currently stored in the InteriorTracker.
fn count_shapes(edges: &[ClippedEdge], face_edges: &[FaceEdge], shape_ids: &[i32]) -> usize {
    let mut count = 0;
    let mut last_shape_id: i32 = -1;

    // index of the current element in the shapeIDs list.
    let mut shape_id_idx = 0;
    for edge in edges {
        let shape_id = face_edges[edge.face_edge].shape_id;
        if shape_id == last_shape_id {
            continue;
        }

        count += 1;
        last_shape_id = shape_id;

        // Skip over any containing shapes up to and including this one,
        // updating count as appropriate.
        while shape_id_idx < shape_ids.len() {
            let clipped_next = shape_ids[shape_id_idx];
            if clipped_next > last_shape_id {
                break;
            }
            if clipped_next < last_shape_id {
                count += 1;
            }
            shape_id_idx += 1;
        }
    }

    // Count any remaining containing shapes.
    count + (shape_ids.len() - shape_id_idx)
}

/// Reports the maximum level for a given edge.
fn max_level_for_edge(edge: Edge) -> i64 {
    // Compute the maximum cell size for which this edge is considered long.
    // The calculation does not need to be perfectly accurate, so we use Norm
    // rather than Angle for speed.
    let cell_size = edge.v0.0.sub(edge.v1.0).norm() * CELL_SIZE_TO_LONG_EDGE_RATIO;
    // Now return the first level encountered during subdivision where the
    // average cell size is at most cellSize.
    AVG_EDGE_METRIC.min_level(cell_size)
}
