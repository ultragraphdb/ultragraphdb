/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/s2.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::errors::types_error::TypesError;
use crate::s2::{EdgeCrosser, Loop, Point};
use crate::types::geometry::Geometry;
use crate::types::geometry::geometry_geojson::GeoJson;
use crate::types::geometry_kind::GeometryKind;
use crate::utils::json;

/// Dgraph's `edgesCrossPoints`: whether an edge of the closed chain `pts` crosses an edge of `l`.
fn edges_cross_points(l: &Loop, pts: &[Point]) -> bool {
    let n = pts.len();
    for i in 0..n {
        let mut crosser = EdgeCrosser::new_chain(pts[i], pts[(i + 1) % n], l.vertex(0));
        for j in 1..=l.num_edges() {
            if crosser.edge_or_vertex_chain_crossing(l.vertex(j)) {
                return true;
            }
        }
    }
    false
}

fn intersects_ordered(l: &Loop, other: &Loop) -> bool {
    if !l.rect_bound().intersects(other.rect_bound()) {
        return false;
    }
    if l.contains_point(other.vertex(0)) {
        return true;
    }
    if edges_cross_points(l, other.vertices()) {
        return true;
    }
    if other.rect_bound().contains(l.rect_bound()) {
        return other.contains_point(l.vertex(0));
    }
    false
}

/// Dgraph's `findVertex`: the index of a vertex of `a` approximately equal to `p`.
fn find_vertex(a: &Loop, p: Point) -> Option<usize> {
    a.vertices().iter().position(|v| v.approx_equal(p))
}

/// Dgraph's `Contains`: whether loop `a` contains loop `b`.
pub fn loop_contains(a: &Loop, b: &Loop) -> bool {
    if !a.rect_bound().contains(b.rect_bound()) {
        return false;
    }
    if !a.contains_point(b.vertex(0)) && find_vertex(a, b.vertex(0)).is_none() {
        return false;
    }
    if edges_cross_points(a, b.vertices()) {
        return false;
    }
    if a.rect_bound().union(b.rect_bound()).is_full()
        && b.contains_point(a.vertex(0))
        && find_vertex(b, a.vertex(0)).is_none()
    {
        return false;
    }
    true
}

/// Dgraph's `Intersects`: whether two loops intersect, testing the edges of the one with more of them.
pub fn loop_intersects(l1: &Loop, l2: &Loop) -> bool {
    if l2.num_edges() > l1.num_edges() {
        return intersects_ordered(l2, l1);
    }
    intersects_ordered(l1, l2)
}

/// Dgraph's `validate` in `convertToGeom`: every polygon's first ring must be closed.
fn validate(g: Geometry) -> Result<Geometry, TypesError> {
    let closed = |p: &Geometry| -> Result<(), TypesError> {
        if p.num_linear_rings() == 0 {
            return Err(TypesError::geo("Got empty polygon."));
        }
        let c = p.linear_ring(0);
        let s = p.stride();
        let l = c.len();
        // Go indexes an empty first ring and panics; this port refuses it as unclosed.
        if l >= s && s >= 2 && c[0] == c[l - s] && c[1] == c[l - s + 1] {
            return Ok(());
        }
        Err(TypesError::geo("Last coord not same as first"))
    };
    match g.kind() {
        GeometryKind::MultiPolygon => {
            (0..g.num_polygons()).try_for_each(|i| closed(&g.polygon_at(i)))?
        }
        GeometryKind::Polygon => closed(&g)?,
        _ => {}
    }
    Ok(g)
}

/// Dgraph's `convertToGeom`: a GeoJSON geometry, or bare coordinates of a point (`[x, y]`), polygon (`[[[...]]]`)
/// or multipolygon (`[[[[...]]]]`), with closed polygons.
pub fn convert_to_geom(text: &str) -> Result<Geometry, TypesError> {
    if let Ok(Ok(g)) = json::parse(text.as_bytes()).map(|v| GeoJson::from_value(&v)) {
        return validate(g.unwrap_or_default().decode()?);
    }
    let s: String = text.chars().filter(|&c| c != ' ' && c != '\t').collect();
    if s.len() < 5 {
        return Err(TypesError::geo("Invalid coordinates"));
    }
    let kind = if s.starts_with("[[[[") {
        "MultiPolygon"
    } else if s.starts_with("[[[") {
        "Polygon"
    } else if s.starts_with('[') {
        "Point"
    } else {
        return Err(TypesError::geo("Invalid coordinates"));
    };
    // Go wraps a polygon's decoding error, not a point's.
    let invalid = |e: TypesError| {
        if kind == "Point" {
            e
        } else {
            e.wrap("Invalid coordinates")
        }
    };
    let coords = json::parse(s.as_bytes()).map_err(|e| invalid(TypesError::geo(e)))?;
    let g = GeoJson {
        kind: kind.into(),
        coordinates: (coords != json::JsonValue::Null).then_some(coords),
        geometries: None,
    };
    let g = g.decode().map_err(invalid)?;
    if kind == "Point" { Ok(g) } else { validate(g) }
}
