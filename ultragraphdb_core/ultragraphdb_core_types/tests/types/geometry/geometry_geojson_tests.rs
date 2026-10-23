/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Golden vectors from `tools/golden/types_geo.go`: go-geom's `geojson.Unmarshal`, Go 1.27's `json.Unmarshal`
//! into `geojson.Geometry`, written back with `geojson.Marshal`.

use serde::Deserialize;
use ultragraphdb_core_types::Geometry;

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

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn from_geojson_matches_go() {
    let path = ultragraphdb_test_bdd::testdata_dir().join("golden/types_geojson.json");
    let golden = ultragraphdb_test_bdd::load_golden::<GeoJsonCase>(&path);
    assert!(golden.cases().len() > 60);
    let failures: Vec<String> = golden
        .cases()
        .iter()
        .filter_map(|c| {
            let input = unhex(&c.input);
            let got = Geometry::from_geojson(&input);
            let ok = match &got {
                Err(e) => e.to_string() == c.err,
                Ok(None) => c.err.is_empty() && c.nil,
                Ok(Some(g)) => {
                    c.err.is_empty() && g.to_geojson().as_deref() == Some(c.out.as_str())
                }
            };
            (!ok).then(|| {
                format!(
                    "{:?}: {:?}, want {:?}",
                    String::from_utf8_lossy(&input),
                    got.map(|g| g.and_then(|g| g.to_geojson())),
                    (&c.err, c.nil, &c.out)
                )
            })
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn nested_collections_decode_without_recursion() {
    // Go nests JSON 10000 deep, so a GeometryCollection may nest 4999 deep; a thread with a small stack decodes it.
    // The geometry is dropped on the test's thread: dropping a Geometry recurses into its members.
    let depth = 4999;
    let input = format!(
        "{}{}{}",
        r#"{"type":"GeometryCollection","geometries":["#.repeat(depth),
        r#"{"type":"Point","coordinates":[1,2]}"#,
        "]}".repeat(depth)
    );
    let got = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || Geometry::from_geojson(input))
        .unwrap()
        .join()
        .unwrap();
    assert!(matches!(got, Ok(Some(_))));
}
