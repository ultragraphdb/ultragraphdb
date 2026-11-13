/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_geo::{GeoErrorEnum, Geometry, Position, Shape};
use ultragraphdb_tool_bdd::TestResult;

use super::fixtures::{one_of_each, positions, ring};

/// Whether `shape` is refused for its shape.
fn refused(shape: Shape) -> bool {
    matches!(Geometry::new(shape), Err(e) if matches!(e.0, GeoErrorEnum::Shape(_)))
}

/// A geometry answers the shape it was built from and that shape's name.
#[test]
fn test_a_geometry_answers_its_shape_and_name() -> TestResult {
    let names: Vec<&str> = one_of_each()?.iter().map(Geometry::name).collect();
    assert_eq!(
        names,
        [
            "Point",
            "LineString",
            "Polygon",
            "MultiPoint",
            "MultiLineString",
            "MultiPolygon",
            "GeometryCollection"
        ]
    );
    let p = Position::new(1.0, 2.0)?;
    assert_eq!(Geometry::point(p).shape(), &Shape::Point(p));
    Ok(())
}

/// The empty multi-geometries and the empty polygon are whole.
#[test]
fn test_empty_geometries_are_built() {
    for shape in [
        Shape::Polygon(Vec::new()),
        Shape::MultiPoint(Vec::new()),
        Shape::MultiLineString(Vec::new()),
        Shape::MultiPolygon(Vec::new()),
        Shape::GeometryCollection(Vec::new()),
    ] {
        assert!(Geometry::new(shape).is_ok());
    }
}

/// A line of fewer than two positions is refused, alone and inside a multi-line.
#[test]
fn test_a_short_line_is_refused() -> TestResult {
    let one = positions(&[(1.0, 2.0)])?;
    assert!(refused(Shape::LineString(Vec::new())));
    assert!(refused(Shape::LineString(one.clone())));
    assert!(refused(Shape::MultiLineString(vec![ring()?, one])));
    Ok(())
}

/// A ring of fewer than four positions, or one that does not end where it starts, is refused.
#[test]
fn test_a_short_or_open_ring_is_refused() -> TestResult {
    let short = positions(&[(0.0, 0.0), (1.0, 1.0), (0.0, 0.0)])?;
    let open = positions(&[(0.0, 0.0), (4.0, 0.0), (4.0, 3.0), (0.0, 1.0)])?;
    assert!(refused(Shape::Polygon(vec![short.clone()])));
    assert!(refused(Shape::Polygon(vec![ring()?, open.clone()])));
    assert!(refused(Shape::MultiPolygon(vec![
        vec![ring()?],
        vec![short]
    ])));
    assert!(refused(Shape::MultiPolygon(vec![vec![open]])));
    Ok(())
}

/// Positions with an elevation beside positions without one are refused in every shape that holds several.
#[test]
fn test_mixed_dimensions_are_refused() -> TestResult {
    let flat = Position::new(1.0, 2.0)?;
    let raised = Position::new(3.0, 4.0)?.with_elevation(5.0)?;
    let raised_ring: Vec<Position> = ring()?
        .into_iter()
        .map(|p| p.with_elevation(1.0))
        .collect::<Result<_, _>>()?;
    assert!(refused(Shape::LineString(vec![flat, raised])));
    assert!(refused(Shape::MultiPoint(vec![raised, flat])));
    assert!(refused(Shape::Polygon(vec![ring()?, raised_ring.clone()])));
    assert!(refused(Shape::MultiLineString(vec![
        ring()?,
        raised_ring.clone()
    ])));
    assert!(refused(Shape::MultiPolygon(vec![
        vec![ring()?],
        vec![raised_ring.clone()]
    ])));
    assert!(Geometry::new(Shape::Polygon(vec![raised_ring])).is_ok());
    Ok(())
}
