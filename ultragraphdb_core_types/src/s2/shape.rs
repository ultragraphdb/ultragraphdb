/*
 * Copyright 2017 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/shape.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::s2::point::Point;

/// An edge of a shape: the endpoints V0 and V1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Edge {
    pub(crate) v0: Point,
    pub(crate) v1: Point,
}

/// The point and its containment for a shape's reference point, from which point
/// containment is computed by counting edge crossings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ReferencePoint {
    pub(crate) point: Point,
    pub(crate) contained: bool,
}

impl ReferencePoint {
    /// Returns a ReferencePoint with the given value for contained and the origin point. It
    /// should be used when all points or no points are contained.
    pub(crate) fn origin(contained: bool) -> ReferencePoint {
        ReferencePoint {
            point: Point::origin(),
            contained,
        }
    }
}

/// The part of golang/geo's Shape interface the ShapeIndex reads: a collection of edges that
/// optionally defines an interior. Implemented statically; the index is built from one shape.
pub(crate) trait Shape {
    /// Returns the number of edges in this shape.
    fn num_edges(&self) -> usize;

    /// Returns the edge for the given edge index.
    fn edge(&self, i: usize) -> Edge;

    /// Returns the dimension of the geometry represented by this shape: 0 for points, 1 for
    /// polylines and 2 for polygons.
    fn dimension(&self) -> i32;

    /// Returns an arbitrary reference point for the shape and whether it is contained.
    fn reference_point(&self) -> ReferencePoint;
}
