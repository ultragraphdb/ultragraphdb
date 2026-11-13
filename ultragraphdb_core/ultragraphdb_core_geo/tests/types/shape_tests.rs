/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_geo::{Position, Shape};
use ultragraphdb_tool_bdd::TestResult;

/// Each shape answers its GeoJSON type name.
#[test]
fn test_each_shape_names_its_geojson_type() -> TestResult {
    let p = Position::new(1.0, 2.0)?;
    for (shape, expected) in [
        (Shape::Point(p), "Point"),
        (Shape::LineString(vec![p, p]), "LineString"),
        (Shape::Polygon(Vec::new()), "Polygon"),
        (Shape::MultiPoint(Vec::new()), "MultiPoint"),
        (Shape::MultiLineString(Vec::new()), "MultiLineString"),
        (Shape::MultiPolygon(Vec::new()), "MultiPolygon"),
        (Shape::GeometryCollection(Vec::new()), "GeometryCollection"),
    ] {
        assert_eq!(shape.name(), expected);
    }
    Ok(())
}
