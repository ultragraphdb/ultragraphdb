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

#[test]
fn test_convert_to_geo_json_point() {
    let b = convert_to_geom("[1.0, 2.0] ").unwrap();
    assert_eq!(b.kind(), GeometryKind::Point);
    assert_eq!(b.coords(), &[1.0, 2.0]);
}

#[test]
fn test_convert_to_geo_json_poly() {
    let b = convert_to_geom("[[[1.123, 2.543], [-3.23, 4.123], [4.43, -6.123], [1.123, 2.543]]]")
        .unwrap();
    assert_eq!(b.kind(), GeometryKind::Polygon);
    assert_eq!(
        b.linear_ring(0),
        &[1.123, 2.543, -3.23, 4.123, 4.43, -6.123, 1.123, 2.543]
    );
    assert!(b.to_geojson().is_some());
}

#[test]
fn test_convert_to_geo_json_poly_error1() {
    assert!(convert_to_geom("[[1.76, -2.234], [3.543, 4.534], [4.54, 6.213]]").is_err());
}

#[test]
fn test_convert_to_geo_json_poly_error2() {
    assert!(convert_to_geom("[[1.253, 2.2343], [-4.563, 6.1231], []]").is_err());
}

fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Geometry {
    Geometry::polygon(
        Layout::XY,
        &[vec![
            vec![x0, y0],
            vec![x1, y0],
            vec![x1, y1],
            vec![x0, y1],
            vec![x0, y0],
        ]],
    )
    .unwrap()
}

#[test]
fn test_loop_contains_and_intersects() {
    let outer = loop_from_polygon(&square(-124.0, 36.0, -121.0, 39.0)).unwrap();
    let inner = loop_from_polygon(&square(-123.0, 37.0, -122.0, 38.0)).unwrap();
    let apart = loop_from_polygon(&square(-120.0, 35.0, -119.0, 36.0)).unwrap();
    assert!(loop_contains(&outer, &inner));
    assert!(!loop_contains(&inner, &outer));
    assert!(loop_intersects(&outer, &inner));
    assert!(!loop_intersects(&outer, &apart));
}
