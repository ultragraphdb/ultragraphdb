/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/cell.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::s2::cap::Cap;
use crate::s2::cellid::{CellId, MAX_LEVEL, cell_id_from_point, ij_level_to_bound_uv};
use crate::s2::gomath;
use crate::s2::latlng::{LatLng, latitude, longitude};
use crate::s2::point::Point;
use crate::s2::r1::R1Interval;
use crate::s2::r2::R2Point;
use crate::s2::r2::R2Rect;
use crate::s2::rect::Rect;
use crate::s2::s1::{Angle, S1Interval};
use crate::s2::stuv::{face_uv_to_xyz, face_xyz_to_uv, u_axis, v_axis};
use core::f64::consts::{FRAC_PI_2, FRAC_PI_4};

const MACHINE_EPSILON: f64 = f64::EPSILON;

/// Go's constant `3 * math.Pi / 4`.
const PI3_4: f64 = f64::from_bits(0x4002d97c7f3321d2);

/// Go's constant `(1.125 + machineEpsilon64) * machineEpsilon64`.
const CONTAINS_POINT_MARGIN: f64 = f64::from_bits(0x3cb2000000000001);

/// An S2 region object that represents a cell. Unlike CellIDs, it supports efficient
/// containment and intersection tests. However, it is also a more expensive representation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    face: usize,
    level: i64,
    orientation: usize,
    id: CellId,
    uv: R2Rect,
}

impl Cell {
    /// Constructs a Cell corresponding to the given CellID.
    pub fn from_cell_id(id: CellId) -> Cell {
        let (f, i, j, o) = id.face_ij_orientation();
        let level = id.level();
        Cell {
            face: f,
            level,
            orientation: o,
            id,
            uv: ij_level_to_bound_uv(i, j, level),
        }
    }

    /// Constructs a cell for the given Point.
    pub fn from_point(p: Point) -> Cell {
        Cell::from_cell_id(cell_id_from_point(p))
    }

    /// Returns the face this cell is on.
    pub fn face(&self) -> usize {
        self.face
    }

    /// Returns the level of this cell.
    pub fn level(&self) -> i64 {
        self.level
    }

    /// Returns the cell's Hilbert curve orientation.
    pub fn orientation(&self) -> usize {
        self.orientation
    }

    /// Returns the CellID this cell represents.
    pub fn id(&self) -> CellId {
        self.id
    }

    /// Returns whether this Cell is a leaf or not.
    pub fn is_leaf(&self) -> bool {
        self.level == MAX_LEVEL
    }

    /// Returns the k-th vertex of the cell (k = 0,1,2,3) in CCW order (lower left, lower right,
    /// upper right, upper left in the UV plane); `None` for any other k, where Go indexes past the vertices and
    /// panics.
    pub fn vertex(&self, k: usize) -> Option<Point> {
        self.vertex_raw(k).map(|v| Point(v.0.normalize()))
    }

    /// Returns the unnormalized k-th vertex of the cell (k = 0,1,2,3) in CCW order; `None` for any other k.
    pub fn vertex_raw(&self, k: usize) -> Option<Point> {
        let v = self.uv.vertices().get(k).copied()?;
        Some(Point(face_uv_to_xyz(self.face, v.x, v.y)))
    }

    /// Returns the bounds of this cell in (u,v)-space.
    pub fn bound_uv(&self) -> R2Rect {
        self.uv
    }

    /// Returns the direction vector corresponding to the center in (s,t)-space of the given
    /// cell. This is the point at which the cell is divided into four subcells; it is not
    /// necessarily the centroid of the cell in (u,v)-space or (x,y,z)-space.
    pub fn center(&self) -> Point {
        Point(self.id.raw_point().normalize())
    }

    /// Reports whether the intersection of this cell and the other cell is not nil.
    pub fn intersects_cell(&self, oc: &Cell) -> bool {
        self.id.intersects(oc.id)
    }

    /// Reports whether this cell contains the other cell.
    pub fn contains_cell(&self, oc: &Cell) -> bool {
        self.id.contains(oc.id)
    }

    /// Computes a covering of the Cell.
    pub fn cell_union_bound(&self) -> Vec<CellId> {
        vec![self.id]
    }

    /// Returns the latitude of the cell vertex in radians given by (i,j), where i and j
    /// indicate the Hi (1) or Lo (0) corner.
    fn latitude(&self, i: usize, j: usize) -> f64 {
        let (u, v) = self.corner(i, j);
        latitude(Point(face_uv_to_xyz(self.face, u, v))).radians()
    }

    /// Returns the longitude of the cell vertex in radians given by (i,j), where i and j
    /// indicate the Hi (1) or Lo (0) corner.
    fn longitude(&self, i: usize, j: usize) -> f64 {
        let (u, v) = self.corner(i, j);
        longitude(Point(face_uv_to_xyz(self.face, u, v))).radians()
    }

    /// The corner `(i, j)`, each 0 for the low and 1 for the high bound; the callers pass only 0 and 1, and any
    /// other value reads as 1, where Go panics.
    fn corner(&self, i: usize, j: usize) -> (f64, f64) {
        let u = if i == 0 { self.uv.x.lo } else { self.uv.x.hi };
        let v = if j == 0 { self.uv.y.lo } else { self.uv.y.hi };
        (u, v)
    }

    /// Returns the bounding rectangle of this cell.
    pub fn rect_bound(&self) -> Rect {
        if self.level > 0 {
            // Except for cells at level 0, the latitude and longitude extremes are
            // attained at the vertices.  Furthermore, the latitude range is
            // determined by one pair of diagonally opposite vertices and the
            // longitude range is determined by the other pair.
            //
            // We first determine which corner (i,j) of the cell has the largest
            // absolute latitude.  To maximize latitude, we want to find the point in
            // the cell that has the largest absolute z-coordinate and the smallest
            // absolute x- and y-coordinates.  To do this we look at each coordinate
            // (u and v), and determine whether we want to minimize or maximize that
            // coordinate based on the axis direction and the cell's (u,v) quadrant.
            let u = self.uv.x.lo + self.uv.x.hi;
            let v = self.uv.y.lo + self.uv.y.hi;
            let mut i = 0;
            let mut j = 0;
            if u_axis(self.face).z == 0.0 {
                if u < 0.0 {
                    i = 1;
                }
            } else if u > 0.0 {
                i = 1;
            }
            if v_axis(self.face).z == 0.0 {
                if v < 0.0 {
                    j = 1;
                }
            } else if v > 0.0 {
                j = 1;
            }
            let lat =
                R1Interval::from_point(self.latitude(i, j)).add_point(self.latitude(1 - i, 1 - j));
            let lng = S1Interval::empty()
                .add_point(self.longitude(i, 1 - j))
                .add_point(self.longitude(1 - i, j));

            // We grow the bounds slightly to make sure that the bounding rectangle
            // contains LatLngFromPoint(P) for any point P inside the loop L defined by the
            // four *normalized* vertices.  Note that normalization of a vector can
            // change its direction by up to 0.5 * dblEpsilon radians, and it is not
            // enough just to add Normalize calls to the code above because the
            // latitude/longitude ranges are not necessarily determined by diagonally
            // opposite vertex pairs after normalization.
            //
            // We would like to bound the amount of normalization error that can be produced
            // by a given value of (u,v), but it is not clear how to do this since
            // we have no a priori knowledge of the face size.
            return Rect::new(lat, lng)
                .expanded(LatLng::new(
                    Angle(2.0 * MACHINE_EPSILON),
                    Angle(2.0 * MACHINE_EPSILON),
                ))
                .polar_closure();
        }

        // The 4 cells around the equator extend to +/-45 degrees latitude at the
        // midpoints of their top and bottom edges.  The two cells covering the
        // poles extend down to +/-35.26 degrees at their vertices.  The maximum
        // error in this calculation is 0.5 * dblEpsilon.
        let pole_min_lat = gomath::asin((1.0f64 / 3.0).sqrt()) - 0.5 * MACHINE_EPSILON;
        let bound = match self.face {
            0 => Rect::new(
                R1Interval::new(-FRAC_PI_4, FRAC_PI_4),
                S1Interval::new(-FRAC_PI_4, FRAC_PI_4),
            ),
            1 => Rect::new(
                R1Interval::new(-FRAC_PI_4, FRAC_PI_4),
                S1Interval::new(FRAC_PI_4, PI3_4),
            ),
            2 => Rect::new(R1Interval::new(pole_min_lat, FRAC_PI_2), S1Interval::full()),
            3 => Rect::new(
                R1Interval::new(-FRAC_PI_4, FRAC_PI_4),
                S1Interval::new(PI3_4, -PI3_4),
            ),
            4 => Rect::new(
                R1Interval::new(-FRAC_PI_4, FRAC_PI_4),
                S1Interval::new(-PI3_4, -FRAC_PI_4),
            ),
            _ => Rect::new(
                R1Interval::new(-FRAC_PI_2, -pole_min_lat),
                S1Interval::full(),
            ),
        };

        // Finally, we expand the bound to account for the error when a point P is
        // converted to an LatLng to test for containment. (The bound should be
        // large enough so that it contains the computed LatLng of any contained
        // point, not just the infinite-precision version.) We don't need to expand
        // longitude because longitude is calculated via a single call to math.Atan2,
        // which is guaranteed to be semi-monotonic.
        bound.expanded(LatLng::new(Angle(MACHINE_EPSILON), Angle(0.0)))
    }

    /// Returns the bounding cap of this cell.
    pub fn cap_bound(&self) -> Cap {
        // We use the cell center in (u,v)-space as the cap axis.  This vector is very close
        // to GetCenter() and faster to compute.  Neither one of these vectors yields the
        // bounding cap with minimal surface area, but they are both pretty close.
        let c = self.uv.center();
        let mut cap = Cap::from_point(Point(face_uv_to_xyz(self.face, c.x, c.y).normalize()));
        for v in (0..4).filter_map(|k| self.vertex(k)) {
            cap = cap.add_point(v);
        }
        cap
    }

    /// Reports whether this cell contains the given point. Note that unlike Loop/Polygon, a
    /// Cell is considered to be a closed set. This means that a point on a Cell's edge or
    /// vertex belong to the Cell and the relevant adjacent Cells too.
    ///
    /// If you want every point to be contained by exactly one Cell, you will need to convert
    /// the Cell to a Loop.
    pub fn contains_point(&self, p: Point) -> bool {
        let Some((u, v)) = face_xyz_to_uv(self.face, p) else {
            return false;
        };

        // Expand the (u,v) bound to ensure that
        //
        //   CellFromPoint(p).ContainsPoint(p)
        //
        // is always true. To do this, we need to account for the error when
        // converting from (u,v) coordinates to (s,t) coordinates. In the
        // normal case the total error is at most dblEpsilon.
        self.uv
            .expanded_by_margin(CONTAINS_POINT_MARGIN)
            .contains_point(R2Point::new(u, v))
    }
}
