/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! go-geom v1.6.1's `wkb.Marshal` and `wkb.Unmarshal`. Every expected byte string and error text is go-geom's,
//! printed by a Go program over the same geometries.

use ultragraphdb_core_types::{Geometry, GeometryKind, Layout};

fn hex(b: &[u8]) -> String {
    b.iter().map(|c| format!("{c:02x}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn ring(points: &[[f64; 2]]) -> Vec<Vec<f64>> {
    points.iter().map(|p| p.to_vec()).collect()
}

const EMPTY_POINT: &str = "cannot encode empty Point in WKB";

/// Each geometry, its little-endian WKB (or go-geom's error) and its big-endian WKB.
fn marshal_cases() -> Vec<(Geometry, Result<&'static str, &'static str>, &'static str)> {
    let square = ring(&[[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 0.0]]);
    let hole = ring(&[[1.0, 1.0], [2.0, 1.0], [2.0, 2.0], [1.0, 1.0]]);
    let unit = ring(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 0.0]]);
    vec![
        (
            Geometry::point(Layout::XY, &[1.0, 2.0]).unwrap(),
            Ok("0101000000000000000000f03f0000000000000040"),
            "00000000013ff00000000000004000000000000000",
        ),
        (
            Geometry::point(Layout::XYM, &[1.0, 2.0, 3.0]).unwrap(),
            Ok("01d1070000000000000000f03f00000000000000400000000000000840"),
            "00000007d13ff000000000000040000000000000004008000000000000",
        ),
        (
            Geometry::point(Layout::XYZM, &[1.0, 2.0, 3.0, 4.0]).unwrap(),
            Ok("01b90b0000000000000000f03f000000000000004000000000000008400000000000001040"),
            "0000000bb93ff0000000000000400000000000000040080000000000004010000000000000",
        ),
        (
            Geometry::line_string(Layout::XY, &ring(&[[0.0, 0.0], [1.0, 1.0]])).unwrap(),
            Ok(
                "01020000000200000000000000000000000000000000000000000000000000f03f000000000000f03f",
            ),
            "000000000200000002000000000000000000000000000000003ff00000000000003ff0000000000000",
        ),
        (
            Geometry::line_string(Layout::XY, &[]).unwrap(),
            Ok("010200000000000000"),
            "000000000200000000",
        ),
        (
            Geometry::polygon(Layout::XY, &[square, hole]).unwrap(),
            Ok(
                "010300000002000000040000000000000000000000000000000000000000000000000010400000000000000000000000000000104000000000000010400000000000000000000000000000000004000000000000000000f03f000000000000f03f0000000000000040000000000000f03f00000000000000400000000000000040000000000000f03f000000000000f03f",
            ),
            "0000000003000000020000000400000000000000000000000000000000401000000000000000000000000000004010000000000000401000000000000000000000000000000000000000000000000000043ff00000000000003ff000000000000040000000000000003ff0000000000000400000000000000040000000000000003ff00000000000003ff0000000000000",
        ),
        (
            Geometry::multi_point(Layout::XY, &[Some(vec![1.0, 2.0]), None]).unwrap(),
            Err(EMPTY_POINT),
            "",
        ),
        (
            Geometry::multi_point(Layout::XY, &[Some(vec![1.0, 2.0]), Some(vec![3.0, 4.0])])
                .unwrap(),
            Ok(
                "0104000000020000000101000000000000000000f03f0000000000000040010100000000000000000008400000000000001040",
            ),
            "00000000040000000200000000013ff00000000000004000000000000000000000000140080000000000004010000000000000",
        ),
        (
            Geometry::multi_line_string(
                Layout::XYZ,
                &[vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]], vec![]],
            )
            .unwrap(),
            Ok(
                "01ed0300000200000001ea03000002000000000000000000f03f0000000000000040000000000000084000000000000010400000000000001440000000000000184001ea03000000000000",
            ),
            "00000003ed0000000200000003ea000000023ff00000000000004000000000000000400800000000000040100000000000004014000000000000401800000000000000000003ea00000000",
        ),
        (
            Geometry::multi_polygon(Layout::XY, &[vec![], vec![unit]]).unwrap(),
            Ok(
                "0106000000020000000103000000000000000103000000010000000400000000000000000000000000000000000000000000000000f03f0000000000000000000000000000f03f000000000000f03f00000000000000000000000000000000",
            ),
            "00000000060000000200000000030000000000000000030000000100000004000000000000000000000000000000003ff000000000000000000000000000003ff00000000000003ff000000000000000000000000000000000000000000000",
        ),
        // A collection without a layout of its own takes its members' widest: XYZ and XYM widen to XYZM.
        (
            Geometry::collection(vec![
                Geometry::point(Layout::XYZ, &[1.0, 2.0, 3.0]).unwrap(),
                Geometry::point(Layout::XYM, &[4.0, 5.0, 6.0]).unwrap(),
            ]),
            Ok(
                "01bf0b00000200000001e9030000000000000000f03f0000000000000040000000000000084001d1070000000000000000104000000000000014400000000000001840",
            ),
            "0000000bbf0000000200000003e93ff00000000000004000000000000000400800000000000000000007d1401000000000000040140000000000004018000000000000",
        ),
        (
            Geometry::collection(Vec::new()),
            Ok("010700000000000000"),
            "000000000700000000",
        ),
        (
            Geometry::collection(vec![Geometry::collection(Vec::new())]),
            Ok("010700000001000000010700000000000000"),
            "000000000700000001000000000700000000",
        ),
        (
            Geometry::point(Layout::new(5), &[1.0, 2.0, 3.0, 4.0, 5.0]).unwrap(),
            Err("geom: unsupported layout Layout(5)"),
            "",
        ),
        (
            Geometry::point_empty(Layout::NO_LAYOUT),
            Err("geom: unsupported layout NoLayout"),
            "",
        ),
        (Geometry::point_empty(Layout::XY), Err(EMPTY_POINT), ""),
        (
            Geometry::collection(vec![Geometry::point_empty(Layout::XY)]),
            Err(EMPTY_POINT),
            "",
        ),
    ]
}

#[test]
fn test_marshal_as_go_geom_writes_it() {
    for (g, want, be) in marshal_cases() {
        let got = g.marshal_wkb();
        assert_eq!(
            got.as_deref().map(hex).map_err(|e| e.to_string()),
            want.map(str::to_string).map_err(str::to_string),
            "{g:?}"
        );
        if let Ok(le) = want {
            // Either byte order reads back to a geometry that writes the same bytes.
            for wkb in [le, be] {
                let back = Geometry::unmarshal_wkb(&unhex(wkb)).unwrap();
                assert_eq!(back.kind(), g.kind(), "{wkb}");
                assert_eq!(hex(&back.marshal_wkb().unwrap()), le, "{wkb}");
            }
        }
    }
}

#[test]
fn test_unmarshal_as_go_geom_reads_it() {
    // Each input and go-geom's result written back as little-endian WKB, or its error.
    type Decoded = (GeometryKind, Layout, &'static str);
    let cases: [(&str, Result<Decoded, &str>); 22] = [
        (
            "01e9030000000000000000f03f00000000000000400000000000000840",
            Ok((
                GeometryKind::Point,
                Layout::XYZ,
                "01e9030000000000000000f03f00000000000000400000000000000840",
            )),
        ),
        // A collection read from WKB keeps its layout.
        (
            "01070000000100000001e9030000000000000000f03f00000000000000400000000000000840",
            Ok((
                GeometryKind::GeometryCollection,
                Layout::XYZ,
                "01ef0300000100000001e9030000000000000000f03f00000000000000400000000000000840",
            )),
        ),
        (
            "010400000000000000",
            Ok((GeometryKind::MultiPoint, Layout::XY, "010400000000000000")),
        ),
        (
            "00000000013ff00000000000004000000000000000",
            Ok((
                GeometryKind::Point,
                Layout::XY,
                "0101000000000000000000f03f0000000000000040",
            )),
        ),
        // NaN coordinates are an ordinary point.
        (
            "0101000000000000000000f8ff000000000000f8ff",
            Ok((
                GeometryKind::Point,
                Layout::XY,
                "0101000000000000000000f8ff000000000000f8ff",
            )),
        ),
        (
            "010600000001000000010300000000000000",
            Ok((
                GeometryKind::MultiPolygon,
                Layout::XY,
                "010600000001000000010300000000000000",
            )),
        ),
        ("", Err("EOF")),
        ("01", Err("EOF")),
        ("010100", Err("unexpected EOF")),
        ("ff", Err("wkb: unknown byte order: 11111111")),
        ("02", Err("wkb: unknown byte order: 10")),
        ("01a10f0000", Err("wkb: unknown type: 4001")),
        ("0107d0070000000000", Err("wkb: unknown type: 512007")),
        ("0108000000", Err("wkb: unsupported type: 8")),
        ("0110000000", Err("wkb: unsupported type: 16")),
        ("01b90b0000", Err("EOF")),
        ("01010000000000", Err("unexpected EOF")),
        ("0104000000", Err("EOF")),
        ("01040000000100", Err("unexpected EOF")),
        // A count larger than the data is refused when the data runs out, without allocating for the count.
        ("0102000000ffffffff", Err("EOF")),
        ("0103000000ffffffff", Err("EOF")),
        (
            "010400000001000000010200000000000000",
            Err("wkb: got *geom.LineString, want *geom.Point"),
        ),
    ];
    for (input, want) in cases {
        let got = Geometry::unmarshal_wkb(&unhex(input))
            .map(|g| (g.kind(), g.layout(), hex(&g.marshal_wkb().unwrap())))
            .map_err(|e| e.to_string());
        assert_eq!(
            got,
            want.map(|(k, l, h)| (k, l, h.to_string()))
                .map_err(str::to_string),
            "{input}"
        );
    }
}

#[test]
fn test_unmarshal_refuses_a_member_of_another_kind_or_layout() {
    let cases = [
        (
            "010500000001000000010300000000000000",
            "wkb: got *geom.Polygon, want *geom.LineString",
        ),
        (
            "010600000001000000010200000000000000",
            "wkb: got *geom.LineString, want *geom.Polygon",
        ),
        (
            "01040000000100000001e9030000000000000000f03f00000000000000400000000000000840",
            "geom: layout mismatch, got XYZ, want XY",
        ),
    ];
    for (input, want) in cases {
        assert_eq!(
            Geometry::unmarshal_wkb(&unhex(input))
                .unwrap_err()
                .to_string(),
            want,
            "{input}"
        );
    }
}
