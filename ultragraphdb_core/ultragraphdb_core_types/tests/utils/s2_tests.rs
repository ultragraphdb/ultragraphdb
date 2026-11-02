/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/s2_test.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_types::{
    Geometry, GeometryKind, Layout, convert_to_geom, loop_contains, loop_from_polygon,
    loop_intersects,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

#[test]
fn test_convert_to_geo_json_point() -> TestResult {
    let b = convert_to_geom("[1.0, 2.0] ")?;
    assert_eq!(b.kind(), GeometryKind::Point);
    assert_eq!(b.coords(), &[1.0, 2.0]);
    Ok(())
}

#[test]
fn test_convert_to_geo_json_poly() -> TestResult {
    let b = convert_to_geom("[[[1.123, 2.543], [-3.23, 4.123], [4.43, -6.123], [1.123, 2.543]]]")?;
    assert_eq!(b.kind(), GeometryKind::Polygon);
    assert_eq!(
        b.linear_ring(0),
        &[1.123, 2.543, -3.23, 4.123, 4.43, -6.123, 1.123, 2.543]
    );
    assert!(b.to_geojson().is_some());
    Ok(())
}

// Both read as a point, `[[` not being `[[[`, and fail as go-geom decodes an array into a float.
#[test]
fn test_convert_to_geo_json_poly_error1() -> TestResult {
    assert_eq!(
        convert_to_geom("[[1.76, -2.234], [3.543, 4.534], [4.54, 6.213]]")
            .err()
            .context("expected an error")?
            .to_string(),
        "json: cannot unmarshal array into Coord.0 of type float64"
    );
    Ok(())
}

#[test]
fn test_convert_to_geo_json_poly_error2() -> TestResult {
    assert_eq!(
        convert_to_geom("[[1.253, 2.2343], [-4.563, 6.1231], []]")
            .err()
            .context("expected an error")?
            .to_string(),
        "json: cannot unmarshal array into Coord.0 of type float64"
    );
    Ok(())
}

/// Dgraph's `convertToGeom` errors, with Go v25.5.0's texts: fewer than five bytes once spaces and tabs are
/// removed, no opening bracket, a polygon's decoding error wrapped and a point's not, and unclosed or empty rings.
#[test]
fn test_convert_to_geom_errors() -> TestResult {
    let cases = [
        ("[1,2", "Invalid coordinates"),
        (" [ 1 , 2", "Invalid coordinates"),
        ("\t[1]\t", "Invalid coordinates"),
        ("abcdef", "Invalid coordinates"),
        (
            "[1,2,]",
            "invalid character ']' looking for beginning of value",
        ),
        (
            "[[[1,2]",
            "Invalid coordinates: unexpected end of JSON input",
        ),
        (
            "[[[[1,2]",
            "Invalid coordinates: unexpected end of JSON input",
        ),
        ("[[[0,0],[1,0],[1,1]]]", "Last coord not same as first"),
        (
            "[[[[0,0],[1,0],[1,1],[0,1]]]]",
            "Last coord not same as first",
        ),
        (
            r#"{"type":"Polygon","coordinates":[]}"#,
            "Got empty polygon.",
        ),
        (
            r#"{"type":"MultiPolygon","coordinates":[[[[0,0],[1,0],[1,1],[0,1]]]]}"#,
            "Last coord not same as first",
        ),
        ("null", "geojson: unsupported type: "),
    ];
    for (input, want) in cases {
        assert_eq!(
            convert_to_geom(input)
                .err()
                .context("expected an error")?
                .to_string(),
            want,
            "{input:?}"
        );
    }
    Ok(())
}

#[test]
fn test_convert_to_geom_shapes() -> TestResult {
    // GeoJSON is decoded as it is; bare coordinates are a point, polygon or multipolygon by their nesting.
    let g = convert_to_geom(r#"{"type":"LineString","coordinates":[[0,0],[1,1]]}"#)?;
    assert_eq!(g.kind(), GeometryKind::LineString);
    assert_eq!(g.flat_coords(), &[0.0, 0.0, 1.0, 1.0]);
    let three_coords = convert_to_geom("\t[ 1 ,\t2 , 3 ]")?;
    assert_eq!(three_coords.kind(), GeometryKind::Point);
    assert_eq!(three_coords.coords(), &[1.0, 2.0, 3.0]);
    let multi_polygon =
        convert_to_geom("[[[[0,0],[1,0],[1,1],[0,0]]],[[[5,5],[6,5],[6,6],[5,5]]]]")?;
    assert_eq!(multi_polygon.kind(), GeometryKind::MultiPolygon);
    assert_eq!(multi_polygon.num_polygons(), 2);
    assert_eq!(
        multi_polygon.polygon_at(1).linear_ring(0),
        &[5.0, 5.0, 6.0, 5.0, 6.0, 6.0, 5.0, 5.0]
    );
    Ok(())
}

fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> TestResult<Geometry> {
    Ok(Geometry::polygon(
        Layout::XY,
        &[vec![
            vec![x0, y0],
            vec![x1, y0],
            vec![x1, y1],
            vec![x0, y1],
            vec![x0, y0],
        ]],
    )?)
}

#[test]
fn test_loop_contains_and_intersects() -> TestResult {
    let outer = loop_from_polygon(&square(-124.0, 36.0, -121.0, 39.0)?)?;
    let inner = loop_from_polygon(&square(-123.0, 37.0, -122.0, 38.0)?)?;
    let apart = loop_from_polygon(&square(-120.0, 35.0, -119.0, 36.0)?)?;
    let crossing = loop_from_polygon(&square(-122.5, 36.5, -120.0, 37.5)?)?;
    assert!(loop_contains(&outer, &inner));
    assert!(!loop_contains(&inner, &outer));
    // As Go answers it: a loop's shared vertices with itself count as crossings, so it does not contain itself.
    assert!(!loop_contains(&outer, &outer));
    assert!(!loop_contains(&outer, &apart));
    assert!(!loop_contains(&outer, &crossing));
    assert!(loop_intersects(&outer, &inner));
    assert!(loop_intersects(&inner, &outer));
    assert!(loop_intersects(&outer, &crossing));
    assert!(loop_intersects(&crossing, &outer));
    assert!(!loop_intersects(&outer, &apart));
    assert!(!loop_intersects(&apart, &outer));
    Ok(())
}
