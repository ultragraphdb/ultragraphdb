/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/value_test.go. Changed for ultragraphdb: translated to Rust; plus the golden
 * TypeForValue vectors (tools/golden/types.go, types_type_for_value).
 * Copyright (c) 2026. Marvin Hansen.
 */

use serde::Deserialize;
use ultragraphdb_core_types::{
    DateTime, TypeId, Val, Value, bytes_as_float_array, float_array_as_bytes,
    float_array_as_string, marshal, type_for_value,
};

#[test]
fn test_type_for_value() {
    let cases = [
        ("true", TypeId::BOOL),
        ("TRUE", TypeId::BOOL),
        ("True", TypeId::BOOL),
        ("t", TypeId::DEFAULT),
        ("false", TypeId::BOOL),
        ("FALSE", TypeId::BOOL),
        ("False", TypeId::BOOL),
        ("f", TypeId::DEFAULT),
        ("2018", TypeId::INT),
        ("2018-10", TypeId::DATETIME),
        ("2018-10-03", TypeId::DATETIME),
        ("2018-10-03T20:47:53Z", TypeId::DATETIME),
        ("123", TypeId::INT),
        ("-123", TypeId::INT),
        ("+123", TypeId::INT),
        ("0001", TypeId::INT),
        ("+0", TypeId::INT),
        ("-0", TypeId::INT),
        ("1World", TypeId::DEFAULT),
        ("3.14159", TypeId::FLOAT),
        ("-273.15", TypeId::FLOAT),
        ("2.99792e8", TypeId::FLOAT),
        ("9.1095E-28", TypeId::FLOAT),
        ("-.0", TypeId::FLOAT),
        ("+.0", TypeId::FLOAT),
        (".1", TypeId::FLOAT),
        ("1.", TypeId::FLOAT),
        ("1-800-4GOLANG", TypeId::DEFAULT),
        ("+1800-446-5264", TypeId::DEFAULT),
        ("212.555.9876", TypeId::DEFAULT),
        ("testing", TypeId::DEFAULT),
    ];
    for (input, want) in cases {
        assert_eq!(type_for_value(input.as_bytes()).0, want, "{input}");
    }
}

/// Go v25.5.0's `TypeForValue` values for inputs at its edges: a sign or dot alone, int64's bounds, a float out of
/// range, and a datetime-shaped text that does not parse, which Go does not retry as an int.
#[test]
fn test_type_for_value_values() {
    let cases = [
        ("", TypeId::DEFAULT, None),
        ("-", TypeId::DEFAULT, None),
        ("+", TypeId::DEFAULT, None),
        (".", TypeId::DEFAULT, None),
        ("9223372036854775808", TypeId::DEFAULT, None),
        (
            "-9223372036854775808",
            TypeId::INT,
            Some(Value::Int(i64::MIN)),
        ),
        ("1e400", TypeId::DEFAULT, None),
        ("2018-13", TypeId::DEFAULT, None),
        (
            "2018-10",
            TypeId::DATETIME,
            Some(Value::DateTime(DateTime::utc(2018, 10, 1, 0, 0, 0, 0))),
        ),
        ("1.", TypeId::FLOAT, Some(Value::Float(1.0))),
        ("-0", TypeId::INT, Some(Value::Int(0))),
        ("+.5", TypeId::FLOAT, Some(Value::Float(0.5))),
        ("1e5", TypeId::FLOAT, Some(Value::Float(100000.0))),
        ("True", TypeId::BOOL, Some(Value::Bool(true))),
        ("FALSE", TypeId::BOOL, Some(Value::Bool(false))),
    ];
    for (input, tid, value) in cases {
        assert_eq!(type_for_value(input.as_bytes()), (tid, value), "{input:?}");
    }
}

#[test]
fn test_bytes_as_float_array_ignores_trailing_bytes() {
    // Go's BytesAsFloatArray converts four bytes at a time and drops the rest.
    assert_eq!(bytes_as_float_array(&[0, 0, 0x80, 0x3f, 9]), [1.0]);
    assert_eq!(bytes_as_float_array(&[1, 2, 3]), [] as [f32; 0]);
    assert_eq!(bytes_as_float_array(&[]), [] as [f32; 0]);
    assert_eq!(
        float_array_as_bytes(&[1.0, -2.0]),
        [0, 0, 0x80, 0x3f, 0, 0, 0, 0xc0]
    );
}

#[test]
#[allow(clippy::excessive_precision)] // Go's test case, which float32 rounds
fn test_float_array_translation() {
    let cases: [&[f32]; 4] = [&[], &[0.1], &[0.0], &[0.65433, 1.855, 3.1415926539]];
    for tc in cases {
        assert_eq!(bytes_as_float_array(&float_array_as_bytes(tc)), tc);
    }
}

#[test]
fn test_float_array_as_string() {
    assert_eq!(
        float_array_as_string(&[0.1, 1.0, -2.5e-7, 3.4e38]),
        "[0.1,1,-0.00000025,340000000000000000000000000000000000000]"
    );
    assert_eq!(float_array_as_string(&[]), "[]");
    // Go 1.27's FormatFloat(float64(f), 'f', -1, 32) of the special and extreme float32 values.
    assert_eq!(
        float_array_as_string(&[
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -0.0,
            f32::from_bits(1),
            f32::MAX,
            16777217.0,
        ]),
        "[NaN,+Inf,-Inf,-0,0.000000000000000000000000000000000000000000001,\
         340282350000000000000000000000000000000,16777216]"
    );
}

#[derive(Deserialize)]
struct Rendered {
    #[serde(default)]
    bin: String,
}

#[derive(Deserialize)]
struct TypeForValueCase {
    #[serde(rename = "in")]
    input: String,
    tid: i32,
    out: Option<Rendered>,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|c| format!("{c:02x}")).collect()
}

#[test]
fn test_golden_type_for_value() {
    let path = ultragraphdb_test_bdd::testdata_dir().join("golden/types_type_for_value.json");
    let golden = ultragraphdb_test_bdd::load_golden::<TypeForValueCase>(&path);
    for c in golden.cases() {
        let (tid, v) = type_for_value(c.input.as_bytes());
        assert_eq!(tid.enum_value(), c.tid, "{}", c.input);
        match (&c.out, v) {
            (None, None) => {}
            (Some(want), Some(v)) => {
                let bin = marshal(&Val::new(tid, v), TypeId::BINARY).unwrap();
                let Some(Value::Bytes(b)) = bin.value() else {
                    panic!("{}", c.input)
                };
                assert_eq!(hex(b), want.bin, "{}", c.input);
            }
            (want, got) => panic!("{}: want value {}, got {got:?}", c.input, want.is_some()),
        }
    }
}
