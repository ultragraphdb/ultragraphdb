/*
 * Copyright 2017 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/shapeutil.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::s2::edge_crosser::EdgeCrosser;
use crate::s2::point::Point;
use crate::s2::shape::Shape;

/// Reports whether the given shape contains the given point. Most clients should not use
/// this method, since its running time is linear in the number of shape edges. Instead
/// clients should create a ShapeIndex and use ContainsPointQuery, since this strategy is
/// much more efficient when many points need to be tested.
///
/// Polygon boundaries are treated as being semi-open (see ContainsPointQuery and
/// VertexModel for other options).
pub(crate) fn contains_brute_force<S: Shape>(shape: &S, point: Point) -> bool {
    if shape.dimension() != 2 {
        return false;
    }

    let ref_point = shape.reference_point();
    if ref_point.point == point {
        return ref_point.contained;
    }

    let mut crosser = EdgeCrosser::new(ref_point.point, point);
    let mut inside = ref_point.contained;
    for e in 0..shape.num_edges() {
        let edge = shape.edge(e);
        inside = inside != crosser.edge_or_vertex_crossing(edge.v0, edge.v1);
    }
    inside
}
