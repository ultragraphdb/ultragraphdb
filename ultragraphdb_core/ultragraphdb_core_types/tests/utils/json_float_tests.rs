/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The float layout of Go 1.27's `json.Marshal`, jsonwire's `AppendFloat` for a float64, reached through
//! `marshal_json` of a float and of a geometry's coordinates. Every expected text is the output of a Go 1.27
//! program marshalling the same float64, whose bits are given beside it where the literal alone is ambiguous.

use ultragraphdb_core_types::{Geometry, Layout, TypeId, Val, Value, marshal_json};

fn json(f: f64) -> Result<String, String> {
    marshal_json(&Val::new(TypeId::FLOAT, Value::Float(f)))
        .map(|b| String::from_utf8(b).unwrap())
        .map_err(|e| e.to_string())
}

#[test]
fn a_float_is_laid_out_as_go_1_27_marshals_it() {
    for (bits, want) in [
        // Zero and negative zero.
        (0x0000000000000000_u64, "0"),
        (0x8000000000000000, "-0"),
        // The smallest subnormals, the largest subnormal and the smallest normal.
        (0x0000000000000001, "5e-324"),
        (0x8000000000000001, "-5e-324"),
        (0x000fffffffffffff, "2.225073858507201e-308"),
        (0x0010000000000000, "2.2250738585072014e-308"),
        // The 1e-6 boundary: `e` below it, `f` from it on.
        (0x3e7ad7f29abcaf48, "1e-7"),
        (0xbe7ad7f29abcaf48, "-1e-7"),
        (0x3eb0c6f7a0b5ed8d, "0.000001"),
        (0xbeb0c6f7a0b5ed8d, "-0.000001"),
        (0x3eb0c6f7a0b5ed8e, "0.0000010000000000000002"),
        (0x01a56e1fc2f8f359, "1e-300"),
        // The 1e21 boundary: `f` below it, `e` from it on.
        (0x4415af1d78b58c40, "100000000000000000000"),
        (0x444b1ae4d6e2ef4f, "999999999999999900000"),
        (0x441ac53a7e04bcda, "123456789012345680000"),
        (0x444b1ae4d6e2ef50, "1e+21"),
        (0xc44b1ae4d6e2ef50, "-1e+21"),
        (0x444b1ae4d6e2ef51, "1.0000000000000001e+21"),
        (0x54b249ad2594c37d, "1e+100"),
        // The extremes, and float32's largest and smallest as float64s.
        (0x7fefffffffffffff, "1.7976931348623157e+308"),
        (0xffefffffffffffff, "-1.7976931348623157e+308"),
        (0x47efffffe0000000, "3.4028234663852886e+38"),
        (0x36a0000000000000, "1.401298464324817e-45"),
        // Ordinary values.
        (0x3ff0000000000000, "1"),
        (0xbff0000000000000, "-1"),
        (0x3fb999999999999a, "0.1"),
        (0x3ff8000000000000, "1.5"),
        (0x4059000000000000, "100"),
        (0x41678c29c0000000, "12345678"),
    ] {
        assert_eq!(
            json(f64::from_bits(bits)),
            Ok(want.to_string()),
            "{bits:#018x}"
        );
    }
}

/// Go 1.27's `UnsupportedValueError` for each value JSON has no number for, its text `%v` of the float.
#[test]
fn a_non_finite_float_is_refused_with_go_1_27s_text() {
    for (f, want) in [
        (f64::NAN, "json: unsupported value: NaN"),
        (f64::INFINITY, "json: unsupported value: +Inf"),
        (f64::NEG_INFINITY, "json: unsupported value: -Inf"),
    ] {
        assert_eq!(json(f), Err(want.to_string()), "{f}");
    }
}

#[test]
fn a_geometrys_coordinates_take_the_same_layout() {
    let p = Geometry::point(Layout::XY, &[-0.0, 9.999999999999999e20]).unwrap();
    assert_eq!(
        String::from_utf8(marshal_json(&Val::new(TypeId::GEO, Value::Geo(p))).unwrap()).unwrap(),
        r#"{"type":"Point","coordinates":[-0,999999999999999900000]}"#
    );
}
