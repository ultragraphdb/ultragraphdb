/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_types::GeometryKind;

#[test]
fn test_geojson_names_and_go_types() {
    let cases = [
        (GeometryKind::Point, "Point", "*geom.Point"),
        (GeometryKind::LineString, "LineString", "*geom.LineString"),
        (GeometryKind::Polygon, "Polygon", "*geom.Polygon"),
        (GeometryKind::MultiPoint, "MultiPoint", "*geom.MultiPoint"),
        (
            GeometryKind::MultiLineString,
            "MultiLineString",
            "*geom.MultiLineString",
        ),
        (
            GeometryKind::MultiPolygon,
            "MultiPolygon",
            "*geom.MultiPolygon",
        ),
        (
            GeometryKind::GeometryCollection,
            "GeometryCollection",
            "*geom.GeometryCollection",
        ),
    ];
    for (kind, name, go_type) in cases {
        assert_eq!(kind.name(), name);
        assert_eq!(kind.go_type(), go_type);
    }
}
