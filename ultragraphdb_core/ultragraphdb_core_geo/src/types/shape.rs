/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::geometry::Geometry;
use crate::types::position::Position;

/// What a [`Geometry`] holds, by GeoJSON's seven types.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// One position.
    Point(Position),
    /// A line through two positions or more.
    LineString(Vec<Position>),
    /// A polygon's rings, the outer first, each closed.
    Polygon(Vec<Vec<Position>>),
    /// Any number of positions.
    MultiPoint(Vec<Position>),
    /// Any number of lines.
    MultiLineString(Vec<Vec<Position>>),
    /// Any number of polygons.
    MultiPolygon(Vec<Vec<Vec<Position>>>),
    /// Any number of geometries, each whole in itself.
    GeometryCollection(Vec<Geometry>),
}

impl Shape {
    /// The shape's GeoJSON type name.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Shape::Point(_) => "Point",
            Shape::LineString(_) => "LineString",
            Shape::Polygon(_) => "Polygon",
            Shape::MultiPoint(_) => "MultiPoint",
            Shape::MultiLineString(_) => "MultiLineString",
            Shape::MultiPolygon(_) => "MultiPolygon",
            Shape::GeometryCollection(_) => "GeometryCollection",
        }
    }
}
