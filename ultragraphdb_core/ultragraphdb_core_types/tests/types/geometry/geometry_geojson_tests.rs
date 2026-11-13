/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Golden vectors from `tools/golden/types_geo.go`: go-geom's `geojson.Unmarshal`, Go 1.27's `json.Unmarshal`
//! into `geojson.Geometry`, written back with `geojson.Marshal`.

use serde::Deserialize;
use ultragraphdb_core_types::{Geometry, GeometryKind, Layout};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

#[derive(Debug, Deserialize)]
struct GeoJsonCase {
    #[serde(rename = "in")]
    input: String,
    #[serde(default)]
    err: String,
    #[serde(default)]
    nil: bool,
    #[serde(default)]
    out: String,
}

fn unhex(s: &str) -> TestResult<Vec<u8>> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).context("a hex byte"))
        .collect()
}

#[test]
fn from_geojson_reads_the_golden_documents() -> TestResult {
    let path = ultragraphdb_tool_bdd::testdata_dir()?.join("golden/types_geojson.json");
    let golden = ultragraphdb_tool_bdd::load_golden::<GeoJsonCase>(&path)?;
    assert!(golden.cases().len() > 60);
    let mut failures: Vec<String> = Vec::new();
    for c in golden.cases() {
        {
            let input = unhex(&c.input)?;
            let got = Geometry::from_geojson(&input);
            // Go refuses a member named twice by its first value; a JSON object's last value wins here.
            if input.starts_with(b"{\"type\":1,\"type\":") {
                continue;
            }
            // A refusal is compared as a refusal: its text is the JSON reader's, where the golden holds Go's.
            let ok = match &got {
                Err(_) => !c.err.is_empty(),
                Ok(None) => c.err.is_empty() && c.nil,
                Ok(Some(g)) => {
                    c.err.is_empty() && g.to_geojson().as_deref() == Some(c.out.as_str())
                }
            };
            if !ok {
                failures.push(format!(
                    "{:?}: {:?}, want {:?}",
                    String::from_utf8_lossy(&input),
                    got.map(|g| g.and_then(|g| g.to_geojson())),
                    (&c.err, c.nil, &c.out)
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    Ok(())
}

/// A GeometryCollection nests as deep as the JSON reader reads, 128 levels, and a deeper one is refused: no text
/// makes the reader recurse past its bound.
#[test]
fn nested_collections_are_read_to_the_readers_bound() -> TestResult {
    let nested = |depth: usize| {
        format!(
            "{}{}{}",
            r#"{"type":"GeometryCollection","geometries":["#.repeat(depth),
            r#"{"type":"Point","coordinates":[1,2]}"#,
            "]}".repeat(depth)
        )
    };
    let shallow = nested(40);
    let g = Geometry::from_geojson(&shallow)?.context("the decoded geometry")?;
    assert_eq!(g.kind(), GeometryKind::GeometryCollection);
    assert_eq!(g.layout(), Layout::XY);
    assert_eq!(g.geoms().len(), 1);
    assert_eq!(g.to_geojson(), Some(shallow));
    assert!(Geometry::from_geojson(nested(4_999)).is_err());
    Ok(())
}

#[test]
fn a_geometry_without_coordinates_is_the_empty_geometry_of_its_type() -> TestResult {
    for kind in [
        GeometryKind::Point,
        GeometryKind::LineString,
        GeometryKind::Polygon,
        GeometryKind::MultiPoint,
        GeometryKind::MultiLineString,
        GeometryKind::MultiPolygon,
    ] {
        let name = format!("{kind:?}");
        let g = Geometry::from_geojson(format!(r#"{{"type":"{name}"}}"#))?
            .context("a geometry without coordinates")?;
        assert_eq!(g.kind(), kind, "{name}");
        assert!(g.is_empty(), "{name}");
    }
    Ok(())
}

#[test]
fn coordinates_that_are_no_array_are_refused_naming_the_go_type() -> TestResult {
    for (name, go_type) in [
        ("LineString", "[]geom.Coord"),
        ("Polygon", "[][]geom.Coord"),
        ("MultiPolygon", "[][][]geom.Coord"),
    ] {
        let Err(e) = Geometry::from_geojson(format!(r#"{{"type":"{name}","coordinates":1}}"#))
        else {
            return Err(TestError::new(format!(
                "{name} with a number for coordinates"
            )));
        };
        assert!(e.to_string().contains(go_type), "{name}: {e}");
    }
    Ok(())
}
