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
    TypeId, Val, Value, bytes_as_float_array, float_array_as_bytes, float_array_as_string, marshal,
    type_for_value,
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
    let path = ultragraphdb_test_bdd::testdata_dir("ultragraphdb_core_types")
        .join("golden/types_type_for_value.json");
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
