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
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

#[test]
fn test_parse() -> TestResult {
    let array = [
        "{'type':'Point','coordinates':[1,2]}",
        "{'type':'MultiLineString','coordinates':[[[1,2,3],[4,5,6],[7,8,9],[1,2,3]]]}",
    ];
    for v in array {
        let g = convert(&Val::bytes(TypeId::STRING, v.as_bytes()), TypeId::GEO)?;
        let text = marshal(&g, TypeId::STRING)?;
        assert_eq!(text.value(), Some(&Value::str(v)));
        let wkb = match marshal(&g, TypeId::BINARY)?.into_value() {
            Some(Value::Bytes(wkb)) => Ok(wkb),
            other => Err(format!("{other:?}")),
        }
        .context("marshalling to binary gives bytes")?;
        let back = convert(&Val::bytes(TypeId::GEO, wkb), TypeId::GEO)?;
        assert_eq!(back, g);
    }
    Ok(())
}

#[test]
fn test_parse_geo_json_errors() -> TestResult {
    // Dgraph v25.5.0's Convert of each string to geo, its error text.
    let cases = [
        (
            r#"{"type":"Curve","coordinates":[1,2]}"#,
            "geojson: unsupported type: Curve",
        ),
        (
            r#"{"type":"Feature","geometry":{"type":"Point","coordinates":[125.6,10.1]},"properties":{"name":"Dinagat Islands"}}"#,
            "geojson: unsupported type: Feature",
        ),
        ("{}", "geojson: unsupported type: "),
        (
            "thisisntjson",
            "invalid character 'h' in literal true (expecting 'r')",
        ),
        ("", "unexpected end of JSON input"),
        (
            r#"{"type":"Point","coordinates":[1]}"#,
            "geojson: dimensionality too low (1)",
        ),
    ];
    for (v, reason) in cases {
        assert_eq!(
            convert(&Val::bytes(TypeId::STRING, v.as_bytes()), TypeId::GEO)
                .err()
                .context("expected an error")?
                .to_string(),
            format!("Error while unmarshalling: [{v}] as geojson: {reason}"),
        );
    }
    Ok(())
}

#[test]
fn test_constructors_refuse_a_coordinate_of_the_wrong_stride() -> TestResult {
    // go-geom's ErrStrideMismatch.
    let cases = [
        (Geometry::point(Layout::XY, &[1.0]), "got 1, want 2"),
        (
            Geometry::point(Layout::XYZ, &[1.0, 2.0, 3.0, 4.0]),
            "got 4, want 3",
        ),
        (Geometry::point(Layout::NO_LAYOUT, &[1.0]), "got 1, want 0"),
        (
            Geometry::line_string(Layout::XYZ, &[vec![1.0, 2.0, 3.0], vec![1.0, 2.0]]),
            "got 2, want 3",
        ),
        (
            Geometry::polygon(Layout::XY, &[vec![vec![0.0, 0.0]], vec![vec![1.0]]]),
            "got 1, want 2",
        ),
        (
            Geometry::multi_point(Layout::XYM, &[None, Some(vec![1.0, 2.0])]),
            "got 2, want 3",
        ),
        (
            Geometry::multi_line_string(Layout::XY, &[vec![vec![1.0, 2.0, 3.0]]]),
            "got 3, want 2",
        ),
        (
            Geometry::multi_polygon(Layout::XYZM, &[vec![vec![vec![1.0, 2.0, 3.0]]]]),
            "got 3, want 4",
        ),
    ];
    for (got, want) in cases {
        assert_eq!(
            got.err().context("expected an error")?.to_string(),
            format!("geom: stride mismatch, {want}")
        );
    }
    Ok(())
}

#[test]
fn test_flat_representation_and_accessors() -> TestResult {
    let p = Geometry::point(Layout::XYZ, &[1.0, 2.0, 3.0])?;
    assert_eq!(
        (p.kind(), p.layout(), p.stride()),
        (GeometryKind::Point, Layout::XYZ, 3)
    );
    assert_eq!(
        (p.coords(), p.flat_coords()),
        (&[1.0, 2.0, 3.0][..], &[1.0, 2.0, 3.0][..])
    );
    assert!(p.ends().is_empty() && p.endss().is_empty() && p.geoms().is_empty());
    assert!(!p.is_empty());
    assert!(Geometry::point_empty(Layout::XY).is_empty());
    assert!(Geometry::line_string(Layout::XY, &[])?.is_empty());

    let rings = [
        vec![
            vec![0.0, 0.0],
            vec![4.0, 0.0],
            vec![4.0, 4.0],
            vec![0.0, 0.0],
        ],
        vec![],
        vec![vec![1.0, 1.0], vec![2.0, 1.0], vec![1.0, 1.0]],
    ];
    let poly = Geometry::polygon(Layout::XY, &rings)?;
    assert_eq!(poly.ends(), &[8, 8, 14]);
    assert_eq!(poly.num_linear_rings(), 3);
    assert_eq!(
        poly.linear_ring(0),
        &[0.0, 0.0, 4.0, 0.0, 4.0, 4.0, 0.0, 0.0]
    );
    assert_eq!(poly.linear_ring(1), &[] as &[f64]);
    assert_eq!(poly.linear_ring(2), &[1.0, 1.0, 2.0, 1.0, 1.0, 1.0]);
    // go-geom indexes past the last ring and panics.
    assert_eq!(poly.linear_ring(3), &[] as &[f64]);
    assert_eq!(poly.linear_ring(usize::MAX), &[] as &[f64]);

    let mpt = Geometry::multi_point(
        Layout::XY,
        &[Some(vec![1.0, 2.0]), None, Some(vec![3.0, 4.0])],
    )?;
    assert_eq!(
        (mpt.flat_coords(), mpt.ends()),
        (&[1.0, 2.0, 3.0, 4.0][..], &[2, 2, 4][..])
    );
    let mls = Geometry::multi_line_string(Layout::XY, &[vec![vec![1.0, 2.0]], vec![]])?;
    assert_eq!(
        (mls.flat_coords(), mls.ends()),
        (&[1.0, 2.0][..], &[2, 2][..])
    );
    assert!(Geometry::multi_point(Layout::XY, &[None])?.is_empty());
    multi_polygon_accessors()?;
    Ok(())
}

/// A multi-polygon's flat ends, polygon count and polygons, the empty ones included.
fn multi_polygon_accessors() -> TestResult {
    let unit = vec![vec![0.0, 0.0], vec![1.0, 0.0], vec![0.0, 0.0]];
    let mp = Geometry::multi_polygon(Layout::XY, &[vec![unit.clone()], vec![], vec![unit]])?;
    assert_eq!(mp.endss(), &[vec![6], vec![], vec![12]]);
    assert_eq!(mp.num_polygons(), 3);
    // The third polygon starts after the last non-empty one before it.
    let third = mp.polygon_at(2);
    assert_eq!(
        (third.kind(), third.ends()),
        (GeometryKind::Polygon, &[6][..])
    );
    assert_eq!(third.flat_coords(), &[0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);
    let empty = mp.polygon_at(1);
    assert!(empty.is_empty() && empty.ends().is_empty());
    // go-geom indexes past the last polygon and panics.
    let past = mp.polygon_at(3);
    assert_eq!(
        (past.kind(), past.layout(), past.is_empty()),
        (GeometryKind::Polygon, Layout::XY, true)
    );
    Ok(())
}

#[test]
fn test_a_collection_takes_its_members_widest_layout() -> TestResult {
    let xy = Geometry::point(Layout::XY, &[1.0, 2.0])?;
    let xyz = Geometry::point(Layout::XYZ, &[1.0, 2.0, 3.0])?;
    let xym = Geometry::point(Layout::XYM, &[1.0, 2.0, 3.0])?;
    let wide = Geometry::point(Layout::new(6), &[0.0; 6])?;
    let cases = [
        (vec![], Layout::NO_LAYOUT),
        (vec![xy.clone()], Layout::XY),
        (vec![xy.clone(), xyz.clone()], Layout::XYZ),
        (vec![xym.clone(), xy.clone()], Layout::XYM),
        (vec![xyz.clone(), xym.clone()], Layout::XYZM),
        (vec![xym.clone(), xyz.clone()], Layout::XYZM),
        (vec![xyz.clone(), wide.clone(), xym.clone()], Layout::new(6)),
        (
            vec![Geometry::collection(vec![xyz.clone()]), xym.clone()],
            Layout::XYZM,
        ),
    ];
    for (members, want) in cases {
        let n = members.len();
        let c = Geometry::collection(members);
        assert_eq!(
            (c.kind(), c.layout(), c.geoms().len()),
            (GeometryKind::GeometryCollection, want, n)
        );
        assert_eq!(c.stride(), want.stride());
    }
    assert!(
        Geometry::collection(vec![Geometry::collection(vec![Geometry::point_empty(
            Layout::XY
        )])])
        .is_empty()
    );
    assert!(!Geometry::collection(vec![Geometry::collection(vec![]), xy]).is_empty());
    Ok(())
}

#[test]
fn test_geojson_errors_carry_go_texts() -> TestResult {
    let err = convert(
        &Val::bytes(TypeId::STRING, br#"{"type":"Curve"}"#.to_vec()),
        TypeId::GEO,
    )
    .err()
    .context("expected an error")?;
    assert_eq!(
        err.to_string(),
        r#"Error while unmarshalling: [{"type":"Curve"}] as geojson: geojson: unsupported type: Curve"#
    );
    let json_err = Geometry::from_geojson("thisisntjson")
        .err()
        .context("expected an error")?;
    assert_eq!(
        json_err.to_string(),
        "invalid character 'h' in literal true (expecting 'r')"
    );
    let coords_err = Geometry::from_geojson(r#"{"type":"Point","coordinates":[1]}"#)
        .err()
        .context("expected an error")?;
    assert_eq!(
        coords_err.to_string(),
        "geojson: dimensionality too low (1)"
    );
    Ok(())
}

#[test]
fn test_wkb_layouts() -> TestResult {
    let p = Geometry::point(Layout::XYZ, &[1.0, 2.0, 3.0])?;
    let wkb = p.marshal_wkb()?;
    assert_eq!(&wkb[..5], &[1, 0xe9, 3, 0, 0]);
    assert_eq!(Geometry::unmarshal_wkb(&wkb)?, p);
    assert_eq!(
        Geometry::point_empty(Layout::XY)
            .marshal_wkb()
            .err()
            .context("expected an error")?
            .to_string(),
        "cannot encode empty Point in WKB"
    );
    assert_eq!(
        Geometry::unmarshal_wkb(&[])
            .err()
            .context("expected an error")?
            .to_string(),
        "EOF"
    );
    assert_eq!(
        Geometry::unmarshal_wkb(&[1, 1, 0])
            .err()
            .context("expected an error")?
            .to_string(),
        "unexpected EOF"
    );
    assert_eq!(
        Geometry::unmarshal_wkb(&[2])
            .err()
            .context("expected an error")?
            .to_string(),
        "wkb: unknown byte order: 10"
    );
    Ok(())
}

#[test]
fn test_geojson_float_layout() -> TestResult {
    let p = Geometry::point(Layout::XY, &[1e-7, 1e21])?;
    assert_eq!(
        p.to_geojson().context("p.to_geojson()")?,
        r#"{"type":"Point","coordinates":[1e-7,1e+21]}"#
    );
    assert!(
        Geometry::point(Layout::XY, &[f64::NAN, 0.0])?
            .to_geojson()
            .is_none()
    );
    Ok(())
}

#[test]
fn test_multi_polygon_members() -> TestResult {
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
    )?;
    assert_eq!(mp.kind(), GeometryKind::MultiPolygon);
    assert_eq!(mp.num_polygons(), 2);
    let p = mp.polygon_at(1);
    assert_eq!(p.linear_ring(0), &[5.0, 5.0, 6.0, 5.0, 6.0, 6.0, 5.0, 5.0]);
    assert_eq!(Geometry::unmarshal_wkb(&mp.marshal_wkb()?)?, mp);
    Ok(())
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
#[expect(
    clippy::disallowed_methods,
    reason = "the test decodes on a small stack to prove the decoder does not recurse"
)]
fn deeply_nested_collections_are_read_written_and_dropped_without_recursion() -> TestResult {
    // WKB nests collections without a limit and go-geom recurses into them on Go's growable stack; on a thread's
    // fixed stack a recursive walk overflows long before 100000 levels.
    let worker = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| -> TestResult {
            let wkb = nested_wkb(100_000);
            let g = Geometry::unmarshal_wkb(&wkb)?;
            assert_eq!(g.kind(), GeometryKind::GeometryCollection);
            assert_eq!(g.layout(), Layout::XY);
            assert!(!g.is_empty());
            assert_eq!(g.marshal_wkb()?, wkb);
            let copy = g.clone();
            assert_eq!(copy.marshal_wkb()?, wkb);
            let text = g.to_geojson().context("the collection's GeoJSON")?;
            assert!(text.starts_with(
                r#"{"type":"GeometryCollection","geometries":[{"type":"GeometryCollection""#
            ));
            let point = r#"{"type":"Point","coordinates":[1,2]}"#;
            let at = text.find(point).context("the innermost point")? + point.len();
            assert_eq!(text[at..], "]}".repeat(100_000));
            drop(copy);
            drop(g);
            Ok(())
        })?;
    worker
        .join()
        .map_err(|_| TestError::new("the worker thread panicked"))??;
    // A shallow collection keeps go-geom's results.
    let g = Geometry::unmarshal_wkb(&nested_wkb(2))?;
    assert_eq!(
        g.to_geojson().context("g.to_geojson()")?,
        r#"{"type":"GeometryCollection","geometries":[{"type":"GeometryCollection","geometries":[{"type":"Point","coordinates":[1,2]}]}]}"#
    );
    let empty = Geometry::unmarshal_wkb(&[1, 7, 0, 0, 0, 0, 0, 0, 0])?;
    assert!(empty.is_empty());
    assert_eq!(empty.marshal_wkb()?, [1, 7, 0, 0, 0, 0, 0, 0, 0]);
    Ok(())
}
