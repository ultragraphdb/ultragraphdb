/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/geofilter_test.go. Changed for ultragraphdb: translated to Rust; plus the golden
 * geo queries (tools/golden/types_geo.go, types_geo_query).
 * Copyright (c) 2026. Marvin Hansen.
 */

use serde::Deserialize;
use ultragraphdb_core_types::{
    GeoQueryData, Geometry, Layout, MAX_CELL_LEVEL, MIN_CELL_LEVEL, QueryType, TypeId, Val, Value,
    convert, get_geo_tokens, match_geo, query_tokens_geo,
};
use ultragraphdb_proto_pb::pb;

use crate::utils::s2index_tests::{load_polygon, unhex};

/// Dgraph's test helper `queryTokens`: the GeoJSON text parsed as geo, then `queryTokensGeo`.
fn query_tokens(
    qt: QueryType,
    data: &str,
    max_distance: f64,
) -> Result<(Vec<String>, GeoQueryData), String> {
    let geo_data = data.replace('\'', "\"");
    let g = match convert(
        &Val::bytes(TypeId::STRING, geo_data.into_bytes()),
        TypeId::GEO,
    ) {
        Ok(v) => match v.into_value() {
            Some(Value::Geo(g)) => g,
            other => panic!("{other:?}"),
        },
        Err(e) => return Err(format!("Cannot decode given geoJson input: {e}")),
    };
    query_tokens_geo(qt, &g, max_distance).map_err(|e| e.to_string())
}

/// Dgraph's `formData`: a geometry's string form, through WKB.
fn form_data(g: &Geometry) -> String {
    let src = Val::bytes(TypeId::GEO, g.marshal_wkb().unwrap());
    match convert(&src, TypeId::STRING).unwrap().into_value() {
        Some(Value::Str(s)) => String::from_utf8(s).unwrap(),
        other => panic!("{other:?}"),
    }
}

fn pt(x: f64, y: f64) -> Geometry {
    Geometry::point(Layout::XY, &[x, y]).unwrap()
}

fn poly(ring: &[[f64; 2]]) -> Geometry {
    Geometry::polygon(Layout::XY, &[ring.iter().map(|c| c.to_vec()).collect()]).unwrap()
}

fn multi(polys: &[&[[f64; 2]]]) -> Geometry {
    Geometry::multi_polygon(
        Layout::XY,
        &polys
            .iter()
            .map(|r| vec![r.iter().map(|c| c.to_vec()).collect()])
            .collect::<Vec<_>>(),
    )
    .unwrap()
}

const SQUARE: [[f64; 2]; 5] = [
    [-122.0, 37.0],
    [-123.0, 37.0],
    [-123.0, 38.0],
    [-122.0, 38.0],
    [-122.0, 37.0],
];

#[test]
fn test_query_tokens_polygon() {
    let data = form_data(&load_polygon("zip.json"));
    for qt in [QueryType::Within, QueryType::Intersects] {
        let (toks, qd) = query_tokens(qt, &data, 0.0).unwrap();
        assert_eq!(toks.len(), if qt == QueryType::Within { 18 } else { 67 });
        assert_eq!(qd.qtype(), qt);
        assert!(!qd.loops().is_empty());
        assert!(qd.pt().is_none());
    }
}

#[test]
fn test_query_tokens_polygon_error() {
    let data = form_data(&load_polygon("zip.json"));
    assert!(query_tokens(QueryType::Near, &data, 0.0).is_err());
}

#[test]
fn test_query_tokens_point() {
    let data = form_data(&pt(-122.082506, 37.4249518));
    let (toks, qd) = query_tokens(QueryType::Contains, &data, 0.0).unwrap();
    assert_eq!(toks.len() as i64, MAX_CELL_LEVEL - MIN_CELL_LEVEL + 1);
    assert_eq!(qd.qtype(), QueryType::Contains);
    assert!(qd.loops().is_empty());
    assert!(qd.pt().is_some());
}

#[test]
fn test_query_tokens_near() {
    let data = form_data(&pt(-122.082506, 37.4249518));
    let (toks, qd) = query_tokens(QueryType::Near, &data, 1000.0).unwrap();
    assert_eq!(toks.len(), 56);
    assert_eq!(qd.qtype(), QueryType::Intersects);
    assert_eq!(qd.loops().len(), 1);
    assert!(qd.pt().is_none());
}

#[test]
fn test_query_tokens_near_error() {
    let data = form_data(&pt(-122.082506, 37.4249518));
    assert!(query_tokens(QueryType::Near, &data, 0.0).is_err());
}

#[test]
fn test_matches_filter_contains_point() {
    let data = form_data(&pt(-122.082506, 37.4249518));
    let (_, qd) = query_tokens(QueryType::Contains, &data, 0.0).unwrap();
    assert!(!qd.matches_filter(&pt(-122.082506, 37.4249518)));
    assert!(qd.matches_filter(&poly(&SQUARE)));
    assert!(!qd.matches_filter(&poly(&[
        [-122.0, 36.0],
        [-123.0, 36.0],
        [-123.0, 37.0],
        [-122.0, 37.0],
        [-122.0, 36.0]
    ])));
    let us = load_polygon("us.json");
    assert!(qd.matches_filter(&us));
    for (x, y, want) in [
        (-157.9197, 21.33, true),
        (-153.369141, 66.160507, true),
        (77.224249103, 28.6077159025, false),
    ] {
        let (_, qd) = query_tokens(QueryType::Contains, &form_data(&pt(x, y)), 0.0).unwrap();
        assert_eq!(qd.matches_filter(&us), want, "{x} {y}");
    }
    let inner: [[f64; 2]; 5] = [
        [-112.0, 39.0],
        [-113.0, 39.0],
        [-113.0, 40.0],
        [-112.0, 40.0],
        [-112.0, 39.0],
    ];
    let (_, qd) = query_tokens(QueryType::Contains, &form_data(&poly(&inner)), 0.0).unwrap();
    assert!(qd.matches_filter(&us));
    let far: [[f64; 2]; 5] = [
        [71.09, 42.35],
        [72.09, 42.35],
        [72.09, 41.35],
        [71.09, 41.35],
        [71.09, 42.35],
    ];
    let (_, qd) = query_tokens(
        QueryType::Contains,
        &form_data(&multi(&[&inner, &far])),
        0.0,
    )
    .unwrap();
    assert!(!qd.matches_filter(&us));
    let sudan = load_polygon("sudan.json");
    let (_, qd) = query_tokens(QueryType::Contains, &form_data(&pt(0.0, 0.0)), 0.0).unwrap();
    assert!(!qd.matches_filter(&sudan));
}

#[test]
fn test_matches_filter_intersects_polygon() {
    let (_, qd) = query_tokens(QueryType::Intersects, &form_data(&poly(&SQUARE)), 0.0).unwrap();
    let p2 = pt(-122.082506, 37.4249518);
    assert!(qd.matches_filter(&p2));
    assert!(!qd.matches_filter(&pt(-123.082506, 37.4249518)));
    assert!(qd.matches_filter(&poly(&[
        [-122.1, 37.1],
        [-122.9, 37.1],
        [-122.9, 37.9],
        [-122.1, 37.9],
        [-122.1, 37.1]
    ])));
    assert!(qd.matches_filter(&poly(&[
        [-121.0, 36.0],
        [-124.0, 36.0],
        [-124.0, 39.0],
        [-121.0, 39.0],
        [-121.0, 36.0]
    ])));
    let crossing: [[f64; 2]; 5] = [
        [-121.5, 36.5],
        [-122.5, 36.5],
        [-122.5, 37.5],
        [-121.5, 37.5],
        [-121.5, 36.5],
    ];
    assert!(qd.matches_filter(&poly(&crossing)));
    assert!(!qd.matches_filter(&poly(&[
        [-120.0, 35.0],
        [-121.0, 35.0],
        [-121.0, 36.0],
        [-120.0, 36.0],
        [-120.0, 35.0]
    ])));
    let poly_out = poly(&[
        [-122.4989104270935, 37.736953437345356],
        [-122.50504732131958, 37.729096212099975],
        [-122.49515533447264, 37.732049133202324],
        [-122.4989104270935, 37.736953437345356],
    ]);
    let poly2 = poly(&[
        [-122.5033039, 37.7334601],
        [-122.503128, 37.7335189],
        [-122.5031222, 37.7335205],
        [-122.5030813, 37.7335868],
        [-122.5031511, 37.73359],
        [-122.5031933, 37.7335916],
        [-122.5032228, 37.7336022],
        [-122.5032697, 37.7335937],
        [-122.5033194, 37.7335874],
        [-122.5033723, 37.7335518],
        [-122.503369, 37.7335068],
        [-122.5033462, 37.7334474],
        [-122.5033039, 37.7334601],
    ]);
    let (_, qd) = query_tokens(QueryType::Intersects, &form_data(&poly_out), 0.0).unwrap();
    assert!(!qd.matches_filter(&poly2));
    let us = load_polygon("us.json");
    let (_, qd) = query_tokens(QueryType::Intersects, &form_data(&poly(&crossing)), 0.0).unwrap();
    assert!(qd.matches_filter(&us));
    let (_, qd) = query_tokens(QueryType::Intersects, &form_data(&us), 0.0).unwrap();
    assert!(qd.matches_filter(&p2));
    assert!(qd.matches_filter(&us));
}

#[test]
fn test_matches_filter_within_polygon() {
    let us = load_polygon("us.json");
    let a: [[f64; 2]; 5] = [
        [-119.53, 37.86],
        [-120.53, 37.86],
        [-120.53, 36.86],
        [-119.53, 36.86],
        [-119.53, 37.86],
    ];
    let (_, qd) = query_tokens(QueryType::Within, &form_data(&us), 0.0).unwrap();
    assert!(qd.matches_filter(&poly(&a)));
    let b: [[f64; 2]; 5] = [
        [-115.13, 36.18],
        [-116.13, 36.18],
        [-116.13, 35.18],
        [-115.13, 35.18],
        [-115.13, 36.18],
    ];
    let c: [[f64; 2]; 5] = [
        [-71.09, 42.35],
        [-72.09, 42.35],
        [-72.09, 41.35],
        [-71.09, 41.35],
        [-71.09, 42.35],
    ];
    // Go's test builds one polygon with three rings; go-geom's MultiPolygon of it holds that one polygon.
    let mp = Geometry::multi_polygon(
        Layout::XY,
        &[[&a, &b, &c]
            .iter()
            .map(|r| r.iter().map(|p| p.to_vec()).collect())
            .collect()],
    )
    .unwrap();
    assert!(qd.matches_filter(&mp));
}

#[test]
fn test_matches_filter_near_point() {
    let (_, qd) = query_tokens(
        QueryType::Near,
        &form_data(&pt(-122.082506, 37.4249518)),
        1000.0,
    )
    .unwrap();
    assert!(qd.matches_filter(&pt(-122.082506, 37.4249518)));
    assert!(qd.matches_filter(&pt(-122.080668, 37.426753)));
    assert!(!qd.matches_filter(&pt(-123.082506, 37.4249518)));
    assert!(qd.matches_filter(&poly(&SQUARE)));
}

#[derive(Deserialize)]
struct GeoQueryCase {
    func: String,
    args: Vec<String>,
    #[serde(default)]
    tokens: Vec<String>,
    #[serde(default)]
    err: String,
    #[serde(default)]
    matches: Vec<bool>,
}

#[derive(Deserialize)]
struct GeoQueryFile {
    candidates: Vec<String>,
    queries: Vec<GeoQueryCase>,
}

#[test]
fn test_golden_geo_queries() {
    let path = ultragraphdb_test_bdd::testdata_dir().join("golden/types_geo_query.json");
    let golden = ultragraphdb_test_bdd::load_golden::<GeoQueryFile>(&path);
    let f = &golden.cases()[0];
    for q in &f.queries {
        let src = pb::SrcFunction {
            name: q.func.clone(),
            args: q.args.clone(),
            is_count: false,
        };
        let ctx = format!(
            "{}({})",
            q.func,
            q.args.join(", ").chars().take(80).collect::<String>()
        );
        match get_geo_tokens(&src) {
            Err(e) => assert_eq!(e.to_string(), q.err, "{ctx}"),
            Ok((mut toks, qd)) => {
                assert!(q.err.is_empty(), "{ctx}: Go refused with {}", q.err);
                toks.sort();
                assert_eq!(toks, q.tokens, "{ctx}");
                let matches: Vec<bool> = f
                    .candidates
                    .iter()
                    .map(|c| {
                        match_geo(
                            &pb::TaskValue {
                                val: unhex(c),
                                val_type: TypeId::GEO.enum_value(),
                            },
                            &qd,
                        )
                    })
                    .collect();
                assert_eq!(matches, q.matches, "{ctx}");
            }
        }
    }
}
