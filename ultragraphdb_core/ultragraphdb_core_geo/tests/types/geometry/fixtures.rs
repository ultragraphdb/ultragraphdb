/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_geo::{GeoError, Geometry, Position, Shape};

/// The positions at the given longitudes and latitudes.
pub fn positions(pairs: &[(f64, f64)]) -> Result<Vec<Position>, GeoError> {
    pairs
        .iter()
        .map(|(lng, lat)| Position::new(*lng, *lat))
        .collect()
}

/// A closed ring of four positions.
pub fn ring() -> Result<Vec<Position>, GeoError> {
    positions(&[(0.0, 0.0), (4.0, 0.0), (4.0, 3.0), (0.0, 0.0)])
}

/// A second closed ring, inside the first.
pub fn hole() -> Result<Vec<Position>, GeoError> {
    positions(&[(1.0, 0.5), (2.0, 0.5), (2.0, 1.0), (1.0, 0.5)])
}

/// One geometry of each of the seven types, the collection holding the six others and an empty collection.
pub fn one_of_each() -> Result<Vec<Geometry>, GeoError> {
    let p = Position::new(8.63, 50.12)?;
    let line = positions(&[(-1.5, 2.25), (3.0, -4.0), (179.0, 89.0)])?;
    let six = vec![
        Geometry::point(p),
        Geometry::new(Shape::LineString(line.clone()))?,
        Geometry::new(Shape::Polygon(vec![ring()?, hole()?]))?,
        Geometry::new(Shape::MultiPoint(line.clone()))?,
        Geometry::new(Shape::MultiLineString(vec![line, ring()?]))?,
        Geometry::new(Shape::MultiPolygon(vec![
            vec![ring()?],
            vec![ring()?, hole()?],
        ]))?,
    ];
    let mut members = six.clone();
    members.push(Geometry::new(Shape::GeometryCollection(Vec::new()))?);
    let collection = Geometry::new(Shape::GeometryCollection(members))?;
    Ok(six.into_iter().chain([collection]).collect())
}

/// One geometry of each type whose positions state an elevation.
pub fn one_of_each_elevated() -> Result<Vec<Geometry>, GeoError> {
    let raise = |ps: Vec<Position>| -> Result<Vec<Position>, GeoError> {
        ps.into_iter().map(|p| p.with_elevation(12.5)).collect()
    };
    let line = raise(positions(&[(-1.5, 2.25), (3.0, -4.0)])?)?;
    let ring = raise(ring()?)?;
    let p = Position::new(8.63, 50.12)?.with_elevation(-3.0)?;
    Ok(vec![
        Geometry::point(p),
        Geometry::new(Shape::LineString(line.clone()))?,
        Geometry::new(Shape::Polygon(vec![ring.clone()]))?,
        Geometry::new(Shape::MultiPoint(line.clone()))?,
        Geometry::new(Shape::MultiLineString(vec![line]))?,
        Geometry::new(Shape::MultiPolygon(vec![vec![ring]]))?,
    ])
}
