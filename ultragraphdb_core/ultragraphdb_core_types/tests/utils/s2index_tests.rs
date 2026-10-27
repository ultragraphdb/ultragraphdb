/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/s2index_test.go. Changed for ultragraphdb: translated to Rust; plus the golden
 * index tokens (tools/golden/types_geo.go, types_geo_index).
 * Copyright (c) 2026. Marvin Hansen.
 */

use serde::Deserialize;
use ultragraphdb_core_types::{
    Geometry, Layout, MAX_CELL_LEVEL, MAX_CELLS, MIN_CELL_LEVEL, TypeId, Val, Value, convert,
    index_cells, index_geo_tokens,
};

/// Dgraph's `loadPolygon`: the geometry of a GeoJSON Feature under Dgraph's `types/testdata`.
pub fn load_polygon(name: &str) -> Geometry {
    let path = ultragraphdb_tool_bdd::testdata_dir().join("geo").join(name);
    let text = std::fs::read_to_string(&path).unwrap();
    let feature: serde_json::Value = serde_json::from_str(&text).unwrap();
    Geometry::from_geojson(feature["geometry"].to_string())
        .unwrap()
        .unwrap()
}

fn point() -> Geometry {
    Geometry::point(Layout::XY, &[-122.082506, 37.4249518]).unwrap()
}

#[test]
fn test_index_cells_point() {
    let (parents, cover) = index_cells(&point()).unwrap();
    assert_eq!(parents.len() as i64, MAX_CELL_LEVEL - MIN_CELL_LEVEL + 1);
    assert_eq!(parents[0].level(), MIN_CELL_LEVEL);
    assert_eq!(parents[0].to_token(), "808c");
    let last = parents[parents.len() - 1];
    assert_eq!(last.level(), MAX_CELL_LEVEL);
    assert_eq!(last.to_token(), "808fb9f81");
    for w in parents.windows(2) {
        assert!(w[1].level() > w[0].level());
    }
    assert_eq!(cover.len(), 1);
    assert_eq!(cover[0].level(), MAX_CELL_LEVEL);
    assert_eq!(cover[0].to_token(), "808fb9f81");
}

#[test]
fn test_index_cells_polygon() {
    let (parents, cover) = index_cells(&load_polygon("zip.json")).unwrap();
    assert!(cover.len() as i64 <= MAX_CELLS);
    for c in &cover {
        assert!((MIN_CELL_LEVEL..=MAX_CELL_LEVEL).contains(&c.level()));
        assert!(parents.contains(c));
    }
    assert!(parents.len() > cover.len());
}

#[test]
fn test_index_cells_polygon_error() {
    let poly = Geometry::polygon(
        Layout::XY,
        &[vec![
            vec![-122.0, 37.0],
            vec![-123.0, 37.0],
            vec![-123.0, 38.0],
            vec![-122.0, 38.0],
            vec![-122.0, 38.0],
        ]],
    )
    .unwrap();
    // Go's %+v of the go-geom polygon, as the golden index vectors hold it.
    assert_eq!(
        index_cells(&poly).unwrap_err().to_string(),
        "Last coordinate not same as first for polygon: &{geom2:{geom1:{geom0:{layout:1 stride:2 \
         flatCoords:[-122 37 -123 37 -123 38 -122 38 -122 38] srid:0}} ends:[10]}}\n"
    );
}

fn round_trip(g: &Geometry) -> Geometry {
    let src = Val::bytes(TypeId::BINARY, g.marshal_wkb().unwrap());
    match convert(&src, TypeId::GEO).unwrap().into_value() {
        Some(Value::Geo(g)) => g,
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_key_generator_point() {
    let keys = index_geo_tokens(&round_trip(&point())).unwrap();
    assert_eq!(keys.len() as i64, MAX_CELL_LEVEL - MIN_CELL_LEVEL + 1 + 1);
}

#[test]
fn test_key_generator_polygon() {
    let keys = index_geo_tokens(&round_trip(&load_polygon("zip.json"))).unwrap();
    assert_eq!(keys.len(), 67);
}

#[derive(Deserialize)]
struct GeoIndexCase {
    name: String,
    wkb: String,
    #[serde(default)]
    tokens: Vec<String>,
    #[serde(default)]
    err: String,
}

pub fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn test_golden_index_tokens() {
    let path = ultragraphdb_tool_bdd::testdata_dir().join("golden/types_geo_index.json");
    let golden = ultragraphdb_tool_bdd::load_golden::<GeoIndexCase>(&path);
    for c in golden.cases() {
        let g = Geometry::unmarshal_wkb(&unhex(&c.wkb)).unwrap();
        match index_geo_tokens(&g) {
            Err(e) => assert_eq!(e.to_string(), c.err, "{}", c.name),
            Ok(mut toks) => {
                toks.sort();
                assert_eq!(toks, c.tokens, "{}", c.name);
            }
        }
    }
}
