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
use ultragraphdb_tool_bdd::{Context, TestResult};

/// Dgraph's `loadPolygon`: the geometry of a GeoJSON Feature under Dgraph's `types/testdata`.
pub fn load_polygon(name: &str) -> TestResult<Geometry> {
    let path = ultragraphdb_tool_bdd::testdata_dir()?
        .join("geo")
        .join(name);
    let text = std::fs::read_to_string(&path)?;
    let feature: serde_json::Value = serde_json::from_str(&text)?;
    Geometry::from_geojson(feature["geometry"].to_string())?.context("the feature's geometry")
}

fn point() -> TestResult<Geometry> {
    Ok(Geometry::point(Layout::XY, &[-122.082506, 37.4249518])?)
}

#[test]
fn test_index_cells_point() -> TestResult {
    let (parents, cover) = index_cells(&point()?)?;
    assert_eq!(
        i64::try_from(parents.len()).context("fits i64")?,
        MAX_CELL_LEVEL - MIN_CELL_LEVEL + 1
    );
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
    Ok(())
}

#[test]
fn test_index_cells_polygon() -> TestResult {
    let (parents, cover) = index_cells(&load_polygon("zip.json")?)?;
    assert!(i64::try_from(cover.len()).context("fits i64")? <= MAX_CELLS);
    for c in &cover {
        assert!((MIN_CELL_LEVEL..=MAX_CELL_LEVEL).contains(&c.level()));
        assert!(parents.contains(c));
    }
    assert!(parents.len() > cover.len());
    Ok(())
}

#[test]
fn test_index_cells_polygon_error() -> TestResult {
    let poly = Geometry::polygon(
        Layout::XY,
        &[vec![
            vec![-122.0, 37.0],
            vec![-123.0, 37.0],
            vec![-123.0, 38.0],
            vec![-122.0, 38.0],
            vec![-122.0, 38.0],
        ]],
    )?;
    // Go's %+v of the go-geom polygon, as the golden index vectors hold it.
    assert_eq!(
        index_cells(&poly)
            .err()
            .context("expected an error")?
            .to_string(),
        "Last coordinate not same as first for polygon: &{geom2:{geom1:{geom0:{layout:1 stride:2 \
         flatCoords:[-122 37 -123 37 -123 38 -122 38 -122 38] srid:0}} ends:[10]}}\n"
    );
    Ok(())
}

fn round_trip(g: &Geometry) -> TestResult<Geometry> {
    let src = Val::bytes(TypeId::BINARY, g.marshal_wkb()?);
    match convert(&src, TypeId::GEO)?.into_value() {
        Some(Value::Geo(geo)) => Ok(geo),
        other => Err(format!("{other:?}")),
    }
    .context("converting to geo gives a geometry")
}

#[test]
fn test_key_generator_point() -> TestResult {
    let keys = index_geo_tokens(&round_trip(&point()?)?)?;
    assert_eq!(
        i64::try_from(keys.len()).context("fits i64")?,
        MAX_CELL_LEVEL - MIN_CELL_LEVEL + 1 + 1
    );
    Ok(())
}

#[test]
fn test_key_generator_polygon() -> TestResult {
    let keys = index_geo_tokens(&round_trip(&load_polygon("zip.json")?)?)?;
    assert_eq!(keys.len(), 67);
    Ok(())
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

pub fn unhex(s: &str) -> TestResult<Vec<u8>> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).context("a hex byte"))
        .collect()
}

#[test]
fn test_golden_index_tokens() -> TestResult {
    let path = ultragraphdb_tool_bdd::testdata_dir()?.join("golden/types_geo_index.json");
    let golden = ultragraphdb_tool_bdd::load_golden::<GeoIndexCase>(&path)?;
    for c in golden.cases() {
        let g = Geometry::unmarshal_wkb(&unhex(&c.wkb)?)?;
        match index_geo_tokens(&g) {
            Err(e) => assert_eq!(e.to_string(), c.err, "{}", c.name),
            Ok(mut toks) => {
                toks.sort();
                assert_eq!(toks, c.tokens, "{}", c.name);
            }
        }
    }
    Ok(())
}
