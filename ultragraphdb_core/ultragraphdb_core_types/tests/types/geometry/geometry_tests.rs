/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/geo_test.go. Changed for ultragraphdb: translated to Rust; plus go-geom's WKB
 * and GeoJSON layouts.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_types::{
    Geometry, GeometryKind, Layout, TypeId, Val, Value, convert, marshal,
};

#[test]
fn test_parse() {
    let array = [
        "{'type':'Point','coordinates':[1,2]}",
        "{'type':'MultiLineString','coordinates':[[[1,2,3],[4,5,6],[7,8,9],[1,2,3]]]}",
    ];
    for v in array {
        let g = convert(&Val::bytes(TypeId::STRING, v.as_bytes()), TypeId::GEO).unwrap();
        let text = marshal(&g, TypeId::STRING).unwrap();
        assert_eq!(text.value(), Some(&Value::str(v)));
        let Some(Value::Bytes(wkb)) = marshal(&g, TypeId::BINARY).unwrap().into_value() else {
            panic!()
        };
        let back = convert(&Val::bytes(TypeId::GEO, wkb), TypeId::GEO).unwrap();
        assert_eq!(back, g);
    }
}

#[test]
fn test_parse_geo_json_errors() {
    let array = [
        r#"{"type":"Curve","coordinates":[1,2]}"#,
        r#"{"type":"Feature","geometry":{"type":"Point","coordinates":[125.6,10.1]},"properties":{"name":"Dinagat Islands"}}"#,
        "{}",
        "thisisntjson",
    ];
    for v in array {
        assert!(
            convert(&Val::bytes(TypeId::STRING, v.as_bytes()), TypeId::GEO).is_err(),
            "{v}"
        );
    }
}

#[test]
fn test_geojson_errors_carry_go_texts() {
    let err = convert(
        &Val::bytes(TypeId::STRING, br#"{"type":"Curve"}"#.to_vec()),
        TypeId::GEO,
    )
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"Error while unmarshalling: [{"type":"Curve"}] as geojson: geojson: unsupported type: Curve"#
    );
    let err = Geometry::from_geojson("thisisntjson").unwrap_err();
    assert_eq!(
        err.to_string(),
        "invalid character 'h' in literal true (expecting 'r')"
    );
    let err = Geometry::from_geojson(r#"{"type":"Point","coordinates":[1]}"#).unwrap_err();
    assert_eq!(err.to_string(), "geojson: dimensionality too low (1)");
}

#[test]
fn test_wkb_layouts() {
    let p = Geometry::point(Layout::XYZ, &[1.0, 2.0, 3.0]).unwrap();
    let wkb = p.marshal_wkb().unwrap();
    assert_eq!(&wkb[..5], &[1, 0xe9, 3, 0, 0]);
    assert_eq!(Geometry::unmarshal_wkb(&wkb).unwrap(), p);
    assert_eq!(
        Geometry::point_empty(Layout::XY)
            .marshal_wkb()
            .unwrap_err()
            .to_string(),
        "cannot encode empty Point in WKB"
    );
    assert_eq!(Geometry::unmarshal_wkb(&[]).unwrap_err().to_string(), "EOF");
    assert_eq!(
        Geometry::unmarshal_wkb(&[1, 1, 0]).unwrap_err().to_string(),
        "unexpected EOF"
    );
    assert_eq!(
        Geometry::unmarshal_wkb(&[2]).unwrap_err().to_string(),
        "wkb: unknown byte order: 10"
    );
}

#[test]
fn test_geojson_float_layout() {
    let p = Geometry::point(Layout::XY, &[1e-7, 1e21]).unwrap();
    assert_eq!(
        p.to_geojson().unwrap(),
        r#"{"type":"Point","coordinates":[1e-7,1e+21]}"#
    );
    assert!(
        Geometry::point(Layout::XY, &[f64::NAN, 0.0])
            .unwrap()
            .to_geojson()
            .is_none()
    );
}

#[test]
fn test_multi_polygon_members() {
    let mp = Geometry::multi_polygon(
        Layout::XY,
        &[
            vec![vec![
                vec![0.0, 0.0],
                vec![1.0, 0.0],
                vec![1.0, 1.0],
                vec![0.0, 0.0],
            ]],
            vec![vec![
                vec![5.0, 5.0],
                vec![6.0, 5.0],
                vec![6.0, 6.0],
                vec![5.0, 5.0],
            ]],
        ],
    )
    .unwrap();
    assert_eq!(mp.kind(), GeometryKind::MultiPolygon);
    assert_eq!(mp.num_polygons(), 2);
    let p = mp.polygon_at(1);
    assert_eq!(p.linear_ring(0), &[5.0, 5.0, 6.0, 5.0, 6.0, 6.0, 5.0, 5.0]);
    assert_eq!(
        Geometry::unmarshal_wkb(&mp.marshal_wkb().unwrap()).unwrap(),
        mp
    );
}

/// WKB of `depth` collections nested one in another around a point.
fn nested_wkb(depth: usize) -> Vec<u8> {
    let mut wkb = Vec::new();
    for _ in 0..depth {
        wkb.extend([1, 7, 0, 0, 0, 1, 0, 0, 0]);
    }
    wkb.extend([1, 1, 0, 0, 0]);
    wkb.extend(1.0f64.to_le_bytes());
    wkb.extend(2.0f64.to_le_bytes());
    wkb
}

#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "the test decodes on a small stack to prove the decoder does not recurse"
)]
fn deeply_nested_collections_are_read_written_and_dropped_without_recursion() {
    // WKB nests collections without a limit and go-geom recurses into them on Go's growable stack; on a thread's
    // fixed stack a recursive walk overflows long before 100000 levels.
    let worker = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let wkb = nested_wkb(100_000);
            let g = Geometry::unmarshal_wkb(&wkb).unwrap();
            assert_eq!(g.kind(), GeometryKind::GeometryCollection);
            assert_eq!(g.layout(), Layout::XY);
            assert!(!g.is_empty());
            assert_eq!(g.marshal_wkb().unwrap(), wkb);
            let copy = g.clone();
            assert_eq!(copy.marshal_wkb().unwrap(), wkb);
            let text = g.to_geojson().unwrap();
            assert!(text.starts_with(
                r#"{"type":"GeometryCollection","geometries":[{"type":"GeometryCollection""#
            ));
            let point = r#"{"type":"Point","coordinates":[1,2]}"#;
            let at = text.find(point).unwrap() + point.len();
            assert_eq!(text[at..], "]}".repeat(100_000));
            drop(copy);
            drop(g);
        })
        .unwrap();
    worker.join().unwrap();
    // A shallow collection keeps go-geom's results.
    let g = Geometry::unmarshal_wkb(&nested_wkb(2)).unwrap();
    assert_eq!(
        g.to_geojson().unwrap(),
        r#"{"type":"GeometryCollection","geometries":[{"type":"GeometryCollection","geometries":[{"type":"Point","coordinates":[1,2]}]}]}"#
    );
    let empty = Geometry::unmarshal_wkb(&[1, 7, 0, 0, 0, 0, 0, 0, 0]).unwrap();
    assert!(empty.is_empty());
    assert_eq!(empty.marshal_wkb().unwrap(), [1, 7, 0, 0, 0, 0, 0, 0, 0]);
}
