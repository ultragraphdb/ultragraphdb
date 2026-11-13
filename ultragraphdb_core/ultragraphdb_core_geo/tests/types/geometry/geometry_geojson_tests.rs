/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_geo::{GeoErrorEnum, Geometry, Position, Shape};
use ultragraphdb_tool_bdd::TestResult;

use super::fixtures::{one_of_each, one_of_each_elevated};

/// Every geometry written as GeoJSON reads back to itself.
#[test]
fn test_every_geometry_round_trips() -> TestResult {
    for geometry in one_of_each()?.into_iter().chain(one_of_each_elevated()?) {
        assert_eq!(Geometry::from_geojson(geometry.to_geojson())?, geometry);
    }
    Ok(())
}

/// The text written is the type, then the coordinates, each number in its shortest digits.
#[test]
fn test_the_text_written_is_exact() -> TestResult {
    for text in [
        r#"{"type":"Point","coordinates":[8.63,50.12]}"#,
        r#"{"type":"Point","coordinates":[8.63,50.12,-3.5]}"#,
        r#"{"type":"LineString","coordinates":[[0,0],[1.5,-2]]}"#,
        r#"{"type":"Polygon","coordinates":[[[0,0],[4,0],[4,3],[0,0]]]}"#,
        r#"{"type":"MultiPoint","coordinates":[[1,2],[3,4]]}"#,
        r#"{"type":"MultiLineString","coordinates":[[[0,0],[1,1]],[[2,2],[3,3]]]}"#,
        r#"{"type":"MultiPolygon","coordinates":[[[[0,0],[4,0],[4,3],[0,0]]],[]]}"#,
        r#"{"type":"GeometryCollection","geometries":[{"type":"Point","coordinates":[1,2]},{"type":"MultiPoint","coordinates":[]}]}"#,
    ] {
        assert_eq!(Geometry::from_geojson(text)?.to_geojson(), text);
    }
    Ok(())
}

/// Members beside the type and coordinates are passed over, and whitespace is free.
#[test]
fn test_other_members_are_passed_over() -> TestResult {
    let text = r#" { "bbox": [0, 0, 1, 1], "coordinates": [ 8.63 , 50.12 ], "crs": null, "type": "Point" } "#;
    assert_eq!(
        Geometry::from_geojson(text)?,
        Geometry::point(Position::new(8.63, 50.12)?)
    );
    Ok(())
}

/// A text that is no JSON document is refused as JSON.
#[test]
fn test_a_text_that_is_no_json_is_refused() {
    for text in ["", "{", r#"{"type":"Point","coordinates":[1,2]"#, "\u{ff}"] {
        assert!(
            matches!(Geometry::from_geojson(text), Err(e) if matches!(e.0, GeoErrorEnum::Json(_))),
            "{text}"
        );
    }
}

/// A document that is no geometry object is refused for its structure.
#[test]
fn test_a_document_that_is_no_geometry_is_refused() {
    for text in [
        "null",
        "[1,2]",
        r#""Point""#,
        r#"{"type":1,"coordinates":[1,2]}"#,
        r#"{"type":"Point"}"#,
        r#"{"type":"GeometryCollection","coordinates":[]}"#,
        r#"{"type":"Point","coordinates":{"lng":1}}"#,
        r#"{"type":"Point","coordinates":[1]}"#,
        r#"{"type":"Point","coordinates":[1,2,3,4]}"#,
        r#"{"type":"Point","coordinates":["1",2]}"#,
        r#"{"type":"Point","coordinates":[null,2]}"#,
        r#"{"type":"LineString","coordinates":[1,2]}"#,
        r#"{"type":"Polygon","coordinates":[[1,2],[3,4]]}"#,
        r#"{"type":"GeometryCollection","geometries":[1]}"#,
    ] {
        assert!(
            matches!(Geometry::from_geojson(text), Err(e) if matches!(e.0, GeoErrorEnum::Structure(_))),
            "{text}"
        );
    }
}

/// A type other than the seven geometry types, or none, is refused by its name.
#[test]
fn test_an_unknown_type_is_refused_by_name() {
    for (text, name) in [
        (r#"{"type":"Feature","geometry":null}"#, "Feature"),
        (r#"{"type":"point","coordinates":[1,2]}"#, "point"),
        (r#"{"coordinates":[1,2]}"#, ""),
    ] {
        assert!(
            matches!(Geometry::from_geojson(text), Err(e) if e.0 == GeoErrorEnum::UnsupportedType(name.to_owned())),
            "{text}"
        );
    }
}

/// What a position and a geometry refuse, the text is refused for.
#[test]
fn test_coordinates_and_shapes_are_checked() {
    assert!(matches!(
        Geometry::from_geojson(r#"{"type":"Point","coordinates":[181,0]}"#),
        Err(e) if matches!(e.0, GeoErrorEnum::Coordinate(_))
    ));
    assert!(matches!(
        Geometry::from_geojson(r#"{"type":"Point","coordinates":[0,1e400]}"#),
        Err(e) if matches!(e.0, GeoErrorEnum::Coordinate(_) | GeoErrorEnum::Json(_))
    ));
    for text in [
        r#"{"type":"LineString","coordinates":[[1,2]]}"#,
        r#"{"type":"Polygon","coordinates":[[[0,0],[4,0],[4,3],[0,1]]]}"#,
        r#"{"type":"MultiPoint","coordinates":[[1,2],[3,4,5]]}"#,
        r#"{"type":"GeometryCollection","geometries":[{"type":"LineString","coordinates":[]}]}"#,
    ] {
        assert!(
            matches!(Geometry::from_geojson(text), Err(e) if matches!(e.0, GeoErrorEnum::Shape(_))),
            "{text}"
        );
    }
}

/// Bytes are read as text is.
#[test]
fn test_bytes_are_read() -> TestResult {
    let read = Geometry::from_geojson(br#"{"type":"MultiPoint","coordinates":[]}"#.as_slice())?;
    assert_eq!(read.shape(), &Shape::MultiPoint(Vec::new()));
    Ok(())
}
