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
    convert, get_geo_tokens, is_geo_func, match_geo, query_tokens_geo,
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
        Some(Value::Str(s)) => s,
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
    assert_eq!(
        query_tokens(QueryType::Near, &data, 0.0).unwrap_err(),
        "Require a point for a within query."
    );
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
    for d in [0.0, -1.0] {
        assert_eq!(
            query_tokens(QueryType::Near, &data, d).unwrap_err(),
            "Invalid max distance specified for a near query",
            "{d}"
        );
    }
}

#[test]
fn test_is_geo_func() {
    // Dgraph's IsGeoFunc matches the four names exactly; GetGeoTokens lowercases, IsGeoFunc does not.
    for (name, want) in [
        ("near", true),
        ("contains", true),
        ("within", true),
        ("intersects", true),
        ("Near", false),
        ("NEAR", false),
        ("", false),
        ("between", false),
        ("near ", false),
    ] {
        assert_eq!(is_geo_func(name), want, "{name:?}");
    }
}

/// Go v25.5.0's `GetGeoTokens` refusals the golden queries leave out, with Go's texts: argument counts, distances
/// `ParseFloat` refuses, polygons `convertToGeom` refuses, and geometries no geo query takes.
#[test]
fn test_get_geo_tokens_refusals() {
    const SQ: &str = "[[[-122, 37], [-123, 37], [-123, 38], [-122, 38], [-122, 37]]]";
    let cases: [(&str, &[&str], &str); 21] = [
        (
            "contains",
            &[SQ, SQ],
            "contains function requires 1 arguments, but got 2",
        ),
        (
            "contains",
            &[],
            "contains function requires 1 arguments, but got 0",
        ),
        (
            "intersects",
            &[],
            "intersects function requires 1 arguments, but got 0",
        ),
        (
            "intersects",
            &[SQ, "1"],
            "intersects function requires 1 arguments, but got 2",
        ),
        (
            "near",
            &["[1,2]", "1", "2"],
            "near function requires 2 arguments, but got 3",
        ),
        (
            "near",
            &["[1,2]", "1e400"],
            "Error while converting distance to float: strconv.ParseFloat: parsing \"1e400\": value out of range",
        ),
        (
            "near",
            &["[1,2]", ""],
            "Error while converting distance to float: strconv.ParseFloat: parsing \"\": invalid syntax",
        ),
        (
            "within",
            &[r#"{"type":"Polygon","coordinates":[]}"#],
            "Got empty polygon.",
        ),
        (
            "within",
            &[r#"{"type":"MultiPolygon","coordinates":[[[[0,0],[1,0],[1,1],[0,1]]]]}"#],
            "Last coord not same as first",
        ),
        (
            "within",
            &["[[[0,0],[1,0],[1,1]]]"],
            "Last coord not same as first",
        ),
        (
            "within",
            &["[[[[0,0],[1,0],[1,1],[0,1]]]]"],
            "Last coord not same as first",
        ),
        (
            "within",
            &["[1, 2, 3]"],
            "Covering only available for 2D co-ordinates.",
        ),
        (
            "within",
            &["[[[1,2]"],
            "Invalid coordinates: unexpected end of JSON input",
        ),
        (
            "within",
            &["[[[[1,2]"],
            "Invalid coordinates: unexpected end of JSON input",
        ),
        ("within", &["[1,2"], "Invalid coordinates"),
        (
            "within",
            &["[1,2,]"],
            "invalid character ']' looking for beginning of value",
        ),
        ("within", &["null"], "geojson: unsupported type: "),
        (
            "within",
            &[r#"{"type":"LineString","coordinates":[[0,0],[1,1]]}"#],
            "Cannot query using a geometry of type *geom.LineString",
        ),
        (
            "within",
            &[r#"{"type":"GeometryCollection","geometries":[]}"#],
            "Cannot query using a geometry of type *geom.GeometryCollection",
        ),
        (
            "contains",
            &[r#"{"type":"MultiPoint","coordinates":[[0,0]]}"#],
            "Cannot query using a geometry of type *geom.MultiPoint",
        ),
        ("", &[SQ], "Invalid function"),
    ];
    let failures: Vec<String> = cases
        .iter()
        .filter_map(|(name, args, want)| {
            let src = pb::SrcFunction {
                name: (*name).into(),
                args: args.iter().map(|a| (*a).into()).collect(),
                is_count: false,
            };
            let got = get_geo_tokens(&src)
                .map(|(t, _)| format!("{} tokens", t.len()))
                .unwrap_or_else(|e| e.to_string());
            (got != *want).then(|| format!("{name}{args:?}: got {got:?}, Go {want:?}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // The function name is lowercased.
    let src = pb::SrcFunction {
        name: "INTERSECTS".into(),
        args: vec![SQ.into()],
        is_count: false,
    };
    assert_eq!(get_geo_tokens(&src).unwrap().0.len(), 47);
}

#[test]
fn test_match_geo_refuses_what_is_not_stored_geo() {
    // Dgraph's MatchGeo: no bytes, another value type, or bytes that are not WKB never match.
    let (_, q) = query_tokens(QueryType::Intersects, &form_data(&poly(&SQUARE)), 0.0).unwrap();
    let inside = pt(-122.5, 37.5).marshal_wkb().unwrap();
    let task = |val: Vec<u8>, tid: TypeId| pb::TaskValue {
        val,
        val_type: tid.enum_value(),
    };
    assert!(match_geo(&task(inside.clone(), TypeId::GEO), &q));
    assert!(!match_geo(&task(inside, TypeId::BINARY), &q));
    assert!(!match_geo(&task(Vec::new(), TypeId::GEO), &q));
    assert!(!match_geo(&task(vec![1, 2, 3], TypeId::GEO), &q));
    let outside = pt(-120.0, 37.5).marshal_wkb().unwrap();
    assert!(!match_geo(&task(outside, TypeId::GEO), &q));
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

#[test]
fn test_geo_inputs_dgraph_panics_on_are_refused() {
    // Dgraph asserts against a function without a name.
    let err = get_geo_tokens(&pb::SrcFunction::default()).unwrap_err();
    assert_eq!(err.to_string(), "Invalid function");
    // A near distance of NaN or infinity passes Go's sign check and reaches exact predicates, where Go's big.Float
    // panics; the port refuses it.
    for d in ["NaN", "+Inf", "Inf"] {
        let src = pb::SrcFunction {
            name: "near".into(),
            args: vec!["[-122.0, 37.0]".into(), d.into()],
            ..Default::default()
        };
        let err = get_geo_tokens(&src).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Invalid max distance specified for a near query",
            "{d}"
        );
    }
    assert_eq!(
        query_tokens_geo(QueryType::Near, &pt(-122.0, 37.0), f64::NAN)
            .unwrap_err()
            .to_string(),
        "Invalid max distance specified for a near query"
    );
    // A polygon of no rings: go-geom's LinearRing(0) indexes past its ends.
    let no_rings = Geometry::polygon(Layout::XY, &[]).unwrap();
    assert_eq!(
        query_tokens_geo(QueryType::Intersects, &no_rings, 0.0)
            .unwrap_err()
            .to_string(),
        "Can't convert ring with less than 4 pts"
    );
    // An empty point: Dgraph's pointFromCoord indexes its missing coordinates.
    let empty = Geometry::point_empty(Layout::XY);
    let err = query_tokens_geo(QueryType::Intersects, &empty, 0.0).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Invalid coordinates []: a point needs two finite numbers"
    );
    assert_eq!(
        ultragraphdb_core_types::index_geo_tokens(&empty)
            .unwrap_err()
            .to_string(),
        "Invalid coordinates []: a point needs two finite numbers"
    );
    // A non-finite coordinate reaches Go's exact predicates, whose big.Float panics on NaN.
    let nan = Geometry::point(Layout::XY, &[f64::NAN, 0.0]).unwrap();
    let err = query_tokens_geo(QueryType::Intersects, &nan, 0.0).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Invalid coordinates [NaN 0]: a point needs two finite numbers"
    );
    let ring = [[0.0, 0.0], [f64::NAN, 0.0], [1.0, 1.0], [0.0, 0.0]];
    assert_eq!(
        query_tokens_geo(QueryType::Intersects, &poly(&ring), 0.0)
            .unwrap_err()
            .to_string(),
        "Invalid coordinates [NaN 0]: a point needs two finite numbers"
    );
    // A stored geometry Dgraph would panic on matches nothing.
    let (_, q) = query_tokens_geo(QueryType::Intersects, &load_polygon("zip.json"), 0.0).unwrap();
    for g in [no_rings, empty, nan, poly(&ring)] {
        assert!(!q.matches_filter(&g), "{g:?}");
    }
}
