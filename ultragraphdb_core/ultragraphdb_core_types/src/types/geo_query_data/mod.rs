/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/geofilter.go (GeoQueryData and its filters). Changed for ultragraphdb:
 * translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::s2::{LatLng, Loop, Point};
use crate::types::geometry::Geometry;
use crate::types::geometry_kind::GeometryKind;
use crate::types::query_type::QueryType;
use crate::utils::s2::{loop_contains, loop_intersects};
use crate::utils::s2index::{loop_from_polygon, point_from_coord};
use crate::utils::xy::is_point_in_ring;

/// Dgraph's `GeoQueryData`: what a geo query's index candidates are filtered by: the query point, the query's loops
/// (a polygon's outer rings, or a near query's circle) and the query type.
#[derive(Debug, Clone)]
pub struct GeoQueryData {
    pt: Option<Point>,
    loops: Vec<Loop>,
    qtype: QueryType,
}

impl GeoQueryData {
    pub(crate) fn new(pt: Option<Point>, loops: Vec<Loop>, qtype: QueryType) -> Self {
        Self { pt, loops, qtype }
    }

    /// The query point, when the query geometry was a point.
    pub fn pt(&self) -> Option<Point> {
        self.pt
    }

    /// The query's loops.
    pub fn loops(&self) -> &[Loop] {
        &self.loops
    }

    /// The query type the filter applies; a near query filters as intersects.
    pub fn qtype(&self) -> QueryType {
        self.qtype
    }

    /// Dgraph's `MatchesFilter`: whether a stored geometry satisfies the query. A geometry Dgraph panics on, an
    /// empty point or one with a coordinate that is not finite, does not.
    pub fn matches_filter(&self, g: &Geometry) -> bool {
        match self.qtype {
            QueryType::Within => self.is_within(g),
            QueryType::Contains => self.contains(g),
            QueryType::Intersects | QueryType::Near => self.intersects(g),
        }
    }

    fn is_within(&self, g: &Geometry) -> bool {
        match g.kind() {
            GeometryKind::Point => {
                // A point Dgraph panics on matches nothing.
                let Ok(p) = point_from_coord(g.coords()) else {
                    return false;
                };
                if let Some(pt) = self.pt {
                    return pt.approx_equal(p);
                }
                self.loops.iter().any(|l| l.contains_point(p))
            }
            GeometryKind::Polygon => match loop_from_polygon(g) {
                Ok(l) => self.loops.iter().any(|q| loop_contains(q, &l)),
                Err(_) => false,
            },
            GeometryKind::MultiPolygon => {
                if self.loops.is_empty() {
                    return false;
                }
                (0..g.num_polygons()).all(|i| match loop_from_polygon(&g.polygon_at(i)) {
                    Ok(l) => self.loops.iter().any(|q| loop_contains(q, &l)),
                    Err(_) => false,
                })
            }
            _ => false,
        }
    }

    fn contains(&self, g: &Geometry) -> bool {
        match g.kind() {
            GeometryKind::Polygon => {
                if let Some(pt) = self.pt {
                    return polygon_contains_coord(g, pt);
                }
                match loop_from_polygon(g) {
                    Ok(s2loop) => self.loops.iter().all(|l| loop_contains(&s2loop, l)),
                    Err(_) => false,
                }
            }
            GeometryKind::MultiPolygon => {
                if let Some(pt) = self.pt {
                    return (0..g.num_polygons())
                        .any(|j| polygon_contains_coord(&g.polygon_at(j), pt));
                }
                if self.loops.is_empty() {
                    return false;
                }
                self.loops.iter().all(|l| multi_polygon_contains_loop(g, l))
            }
            _ => false,
        }
    }

    fn intersects(&self, g: &Geometry) -> bool {
        match g.kind() {
            GeometryKind::Point => match point_from_coord(g.coords()) {
                Ok(p) => self.loops.iter().any(|l| l.contains_point(p)),
                // A point Dgraph panics on matches nothing.
                Err(_) => false,
            },
            GeometryKind::Polygon => match loop_from_polygon(g) {
                Ok(l) => self.loops.iter().any(|q| loop_intersects(&l, q)),
                Err(_) => false,
            },
            GeometryKind::MultiPolygon => {
                for i in 0..g.num_polygons() {
                    let Ok(l) = loop_from_polygon(&g.polygon_at(i)) else {
                        return false;
                    };
                    if self.loops.iter().any(|q| loop_intersects(&l, q)) {
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }
}

/// Dgraph's `multiPolygonContainsLoop`.
fn multi_polygon_contains_loop(g: &Geometry, l: &Loop) -> bool {
    for i in 0..g.num_polygons() {
        let Ok(s2loop) = loop_from_polygon(&g.polygon_at(i)) else {
            return false;
        };
        if loop_contains(&s2loop, l) {
            return true;
        }
    }
    false
}

/// Dgraph's `polygonContainsCoord`: whether any ring of the polygon holds the point, by the planar ray test.
fn polygon_contains_coord(v: &Geometry, pt: Point) -> bool {
    let ll = LatLng::from_point(pt);
    let p = [ll.lng().degrees(), ll.lat().degrees()];
    (0..v.num_linear_rings()).any(|i| is_point_in_ring(v.stride(), p, v.linear_ring(i)))
}
