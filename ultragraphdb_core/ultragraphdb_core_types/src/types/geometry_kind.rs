/*
 * Copyright (c) 2013, Tom Payne. All rights reserved.
 * SPDX-License-Identifier: BSD-2-Clause
 *
 * Ported from go-geom v1.6.1 (the geometry types). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// The go-geom geometry types Dgraph stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GeometryKind {
    /// A single position.
    Point,
    /// A sequence of positions joined by segments.
    LineString,
    /// An outer ring with optional holes.
    Polygon,
    /// A set of points.
    MultiPoint,
    /// A set of line strings.
    MultiLineString,
    /// A set of polygons.
    MultiPolygon,
    /// A set of geometries of any kind.
    GeometryCollection,
}

impl GeometryKind {
    /// The GeoJSON type name.
    pub fn name(self) -> &'static str {
        match self {
            GeometryKind::Point => "Point",
            GeometryKind::LineString => "LineString",
            GeometryKind::Polygon => "Polygon",
            GeometryKind::MultiPoint => "MultiPoint",
            GeometryKind::MultiLineString => "MultiLineString",
            GeometryKind::MultiPolygon => "MultiPolygon",
            GeometryKind::GeometryCollection => "GeometryCollection",
        }
    }

    /// Go's `%T` of the go-geom value, as Dgraph's messages print it.
    pub fn go_type(self) -> &'static str {
        match self {
            GeometryKind::Point => "*geom.Point",
            GeometryKind::LineString => "*geom.LineString",
            GeometryKind::Polygon => "*geom.Polygon",
            GeometryKind::MultiPoint => "*geom.MultiPoint",
            GeometryKind::MultiLineString => "*geom.MultiLineString",
            GeometryKind::MultiPolygon => "*geom.MultiPolygon",
            GeometryKind::GeometryCollection => "*geom.GeometryCollection",
        }
    }
}
