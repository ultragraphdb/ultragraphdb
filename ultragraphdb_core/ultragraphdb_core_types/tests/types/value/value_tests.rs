/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The Go values a `Val` holds, and their `%T`.

use ultragraphdb_core_types::{BIG_FLOAT_PRECISION, BigFloat, DateTime, Geometry, Layout, Value};

#[test]
fn test_str() {
    for s in ["", "abc", "日本", "a\0b"] {
        assert_eq!(Value::str(s), Value::Str(s.to_string()));
    }
}

#[test]
fn test_go_type_is_gos_percent_t() {
    let point = Geometry::point(Layout::XY, &[1.0, 2.0]).unwrap();
    let line = Geometry::line_string(Layout::XY, &[vec![0.0, 0.0], vec![1.0, 1.0]]).unwrap();
    let cases = [
        (Value::Bytes(vec![1]), "[]uint8"),
        (Value::str("x"), "string"),
        (Value::Int(1), "int64"),
        (Value::Float(1.0), "float64"),
        (Value::Bool(true), "bool"),
        (Value::DateTime(DateTime::zero()), "time.Time"),
        (Value::Geo(point), "*geom.Point"),
        (Value::Geo(line), "*geom.LineString"),
        (
            Value::Geo(Geometry::collection(Vec::new())),
            "*geom.GeometryCollection",
        ),
        (Value::Uid(1), "uint64"),
        (
            Value::BigFloat(BigFloat::zero(BIG_FLOAT_PRECISION)),
            "big.Float",
        ),
        (Value::VFloat(vec![1.0]), "[]float32"),
    ];
    for (v, want) in cases {
        assert_eq!(v.go_type(), want, "{v:?}");
    }
}
