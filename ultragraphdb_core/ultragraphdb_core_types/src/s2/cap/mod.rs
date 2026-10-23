/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/cap.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::s2::cellid::{CellId, NUM_FACES, cell_id_from_point};
use crate::s2::metric::MIN_WIDTH_METRIC;
use crate::s2::point::{Point, chord_angle_between_points};
use crate::s2::r3::R3Vector;
use crate::s2::s1::{Angle, ChordAngle};

/// The center point Go's empty and full caps use: PointFromCoords(1.0, 0, 0).
const CENTER_POINT: Point = Point(R3Vector::new(1.0, 0.0, 0.0));

/// A disc-shaped region defined by a center and radius. Technically this shape is called a
/// "spherical cap" (rather than disc) because it is not planar; the cap represents a portion
/// of the sphere that has been cut off by a plane. The boundary of the cap is the circle
/// defined by the intersection of the sphere and the plane. For containment purposes, the cap
/// is a closed set, i.e. it contains its boundary.
///
/// The radius is stored as a ChordAngle, the squared chord length between the center and
/// the boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cap {
    center: Point,
    radius: ChordAngle,
}

impl Cap {
    /// Constructs a cap containing a single point.
    pub fn from_point(p: Point) -> Cap {
        Cap::from_center_chord_angle(p, ChordAngle(0.0))
    }

    /// Constructs a cap with the given center and angle.
    pub fn from_center_angle(center: Point, angle: Angle) -> Cap {
        Cap::from_center_chord_angle(center, ChordAngle::from_angle(angle))
    }

    /// Constructs a cap where the angle is expressed as a ChordAngle. This constructor is more
    /// efficient than using an Angle.
    pub fn from_center_chord_angle(center: Point, radius: ChordAngle) -> Cap {
        Cap { center, radius }
    }

    /// Returns a cap that contains no points.
    pub fn empty() -> Cap {
        Cap::from_center_chord_angle(CENTER_POINT, ChordAngle::NEGATIVE)
    }

    /// Returns a cap that contains all points.
    pub fn full() -> Cap {
        Cap::from_center_chord_angle(CENTER_POINT, ChordAngle::STRAIGHT)
    }

    /// Reports whether the cap is empty, i.e. it contains no points.
    pub fn is_empty(&self) -> bool {
        self.radius.0 < 0.0
    }

    /// Reports whether the cap is full, i.e. it contains all points.
    pub fn is_full(&self) -> bool {
        self.radius == ChordAngle::STRAIGHT
    }

    /// Returns the cap's center point.
    pub fn center(&self) -> Point {
        self.center
    }

    /// Returns the height of the cap. This is the distance from the center point to the
    /// cutoff plane.
    pub fn height(&self) -> f64 {
        0.5 * self.radius.0
    }

    /// Returns the cap radius as an s1.Angle. (Note that the cap angle is stored internally as
    /// a ChordAngle, so this method requires a trigonometric operation and may yield a slightly
    /// different result than the value passed to CapFromCenterAngle).
    pub fn radius(&self) -> Angle {
        self.radius.angle()
    }

    /// Returns the cap radius as the ChordAngle it is stored as.
    pub fn chord_radius(&self) -> ChordAngle {
        self.radius
    }

    /// Reports whether the cap contains the point.
    pub fn contains_point(&self, p: Point) -> bool {
        chord_angle_between_points(self.center, p) <= self.radius
    }

    /// Returns the complement of the interior of the cap. A cap and its complement have the
    /// same boundary but do not share any interior points. The complement operator is not a
    /// bijection because the complement of a singleton cap (containing a single point) is the
    /// same as the complement of an empty cap.
    pub fn complement(&self) -> Cap {
        if self.is_full() {
            return Cap::empty();
        }
        if self.is_empty() {
            return Cap::full();
        }

        Cap::from_center_chord_angle(
            Point(self.center.0.mul(-1.0)),
            ChordAngle::STRAIGHT.sub(self.radius),
        )
    }

    /// Increases the size of the cap to include the given point.
    pub fn add_point(&self, p: Point) -> Cap {
        let mut c = *self;
        if c.is_empty() {
            c.center = p;
            c.radius = ChordAngle(0.0);
            return c;
        }

        // After calling cap.AddPoint(p), cap.Contains(p) must be true. However
        // we don't need to do anything special to achieve this because Contains()
        // does exactly the same distance calculation that we do here.
        let new_rad = chord_angle_between_points(c.center, p);
        if new_rad > c.radius {
            c.radius = new_rad;
        }
        c
    }

    /// Increases the size of the cap to include the given other cap.
    pub fn add_cap(&self, other: Cap) -> Cap {
        let mut c = *self;
        if c.is_empty() {
            return other;
        }
        if other.is_empty() {
            return c;
        }

        // We round up the distance to ensure that the cap is actually contained.
        // TODO(roberts): Do some error analysis in order to guarantee this.
        let dist = chord_angle_between_points(c.center, other.center).add(other.radius);
        let new_rad = dist.expanded(f64::EPSILON * dist.0);
        if new_rad > c.radius {
            c.radius = new_rad;
        }
        c
    }

    /// Computes a covering of the Cap. In general the covering consists of at most 4 cells
    /// except for very large caps, which may need up to 6 cells. The output is not sorted.
    pub fn cell_union_bound(&self) -> Vec<CellId> {
        // TODO(roberts): The covering could be made quite a bit tighter by mapping
        // the cap to a rectangle in (i,j)-space and finding a covering for that.

        // Find the maximum level such that the cap contains at most one cell vertex
        // and such that CellID.AppendVertexNeighbors() can be called.
        let level = MIN_WIDTH_METRIC.max_level(self.radius().radians()) - 1;

        // If level < 0, more than three face cells are required.
        if level < 0 {
            return (0..NUM_FACES).map(CellId::from_face).collect();
        }
        // The covering consists of the 4 cells at the given level that share the
        // cell vertex that is closest to the cap center.
        cell_id_from_point(self.center).vertex_neighbors(level)
    }
}
