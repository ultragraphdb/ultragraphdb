/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/conversion_test.go. Changed for ultragraphdb: translated to Rust; plus the
 * golden conversion matrix (tools/golden/types.go, types_convert).
 * Copyright (c) 2026. Marvin Hansen.
 */

use serde::Deserialize;
use ultragraphdb_core_types::{
    DateTime, TypeId, Val, Value, Zone, convert, float_array_as_bytes, marshal, verify_password,
};

fn bs_bool(b: bool) -> Vec<u8> {
    vec![u8::from(b)]
}

fn bs_i64(v: i64) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn bs_f64(v: f64) -> Vec<u8> {
    v.to_bits().to_le_bytes().to_vec()
}

fn bs_time(t: DateTime) -> Vec<u8> {
    t.marshal_binary().unwrap()
}

fn s(v: &str) -> Value {
    Value::str(v)
}

fn conv(tid: TypeId, data: Vec<u8>, to: TypeId) -> Result<Val, String> {
    convert(&Val::bytes(tid, data), to).map_err(|e| e.to_string())
}

fn ok(tid: TypeId, v: Value) -> Result<Val, String> {
    Ok(Val::new(tid, v))
}

#[allow(clippy::too_many_arguments)]
fn date(y: i64, mo: i64, d: i64, h: i64, mi: i64, se: i64, ns: i64) -> DateTime {
    DateTime::utc(y, mo, d, h, mi, se, ns)
}

#[test]
fn test_same_conversion_string() {
    for input in ["a", ""] {
        assert_eq!(
            conv(TypeId::BINARY, input.into(), TypeId::STRING),
            ok(TypeId::STRING, s(input))
        );
    }
}

#[test]
fn test_same_conversion_date_time() {
    for t in [date(2006, 1, 2, 15, 4, 5, 0), DateTime::zero()] {
        assert_eq!(
            conv(TypeId::BINARY, bs_time(t), TypeId::DATETIME),
            ok(TypeId::DATETIME, Value::DateTime(t))
        );
    }
}

/// From type, from bytes (`None`: Go's nil value), to type, and the failure's substring or the expected value.
type EdgeCase = (TypeId, Option<Vec<u8>>, TypeId, Result<Value, &'static str>);

#[test]
fn test_conversion_edge_cases() {
    // (from type, from bytes or None, to type, failure substring or expected value)
    let zero = DateTime::zero();
    let cases: Vec<EdgeCase> = vec![
        (
            TypeId::BINARY,
            None,
            TypeId::BINARY,
            Err("invalid data to convert to binary"),
        ),
        (
            TypeId::BINARY,
            Some(vec![]),
            TypeId::INT,
            Err("invalid data for int64"),
        ),
        (
            TypeId::BINARY,
            Some(vec![]),
            TypeId::FLOAT,
            Err("invalid data for float"),
        ),
        (
            TypeId::BINARY,
            Some(vec![]),
            TypeId::BOOL,
            Ok(Value::Bool(false)),
        ),
        (
            TypeId::BINARY,
            Some(vec![2]),
            TypeId::BOOL,
            Err("invalid value for bool"),
        ),
        (
            TypeId::BINARY,
            Some(vec![8]),
            TypeId::DATETIME,
            Err("Time.UnmarshalBinary:"),
        ),
        (
            TypeId::BINARY,
            Some(vec![]),
            TypeId::DATETIME,
            Err("Time.UnmarshalBinary:"),
        ),
        (
            TypeId::STRING,
            Some(vec![]),
            TypeId::INT,
            Err("strconv.ParseInt"),
        ),
        (
            TypeId::STRING,
            Some(vec![]),
            TypeId::FLOAT,
            Err("strconv.ParseFloat"),
        ),
        (
            TypeId::STRING,
            Some(vec![]),
            TypeId::BOOL,
            Err("strconv.ParseBool"),
        ),
        (
            TypeId::STRING,
            Some(vec![]),
            TypeId::DATETIME,
            Err(r#"parsing time "" as "2006-01-02T15:04:05": cannot parse "" as "2006""#),
        ),
        (
            TypeId::INT,
            Some(vec![]),
            TypeId::DEFAULT,
            Err("invalid data for int64"),
        ),
        (
            TypeId::INT,
            Some(bs_i64(0)),
            TypeId::DATETIME,
            Ok(Value::DateTime(DateTime::from_unix(0, 0))),
        ),
        (
            TypeId::FLOAT,
            Some(vec![]),
            TypeId::DEFAULT,
            Err("invalid data for float"),
        ),
        (
            TypeId::FLOAT,
            Some(bs_f64(0.0)),
            TypeId::DATETIME,
            Ok(Value::DateTime(DateTime::from_unix(0, 0))),
        ),
        (
            TypeId::BOOL,
            Some(vec![]),
            TypeId::DEFAULT,
            Err("invalid value for bool"),
        ),
        (
            TypeId::BOOL,
            Some(vec![8]),
            TypeId::DEFAULT,
            Err("invalid value for bool"),
        ),
        (
            TypeId::DATETIME,
            Some(vec![]),
            TypeId::DATETIME,
            Err("Time.UnmarshalBinary: no data"),
        ),
        (
            TypeId::DATETIME,
            Some(bs_time(zero)),
            TypeId::DATETIME,
            Ok(Value::DateTime(zero)),
        ),
        (
            TypeId::DATETIME,
            Some(vec![]),
            TypeId::BINARY,
            Err("Time.UnmarshalBinary: no data"),
        ),
        (
            TypeId::DATETIME,
            Some(bs_time(zero)),
            TypeId::BINARY,
            Ok(Value::Bytes(bs_time(zero))),
        ),
        (
            TypeId::DATETIME,
            Some(vec![]),
            TypeId::STRING,
            Err("Time.UnmarshalBinary: no data"),
        ),
        (
            TypeId::DATETIME,
            Some(bs_time(zero)),
            TypeId::STRING,
            Ok(s("0001-01-01T00:00:00Z")),
        ),
        (
            TypeId::DATETIME,
            Some(vec![]),
            TypeId::DEFAULT,
            Err("Time.UnmarshalBinary: no data"),
        ),
        (
            TypeId::DATETIME,
            Some(bs_time(zero)),
            TypeId::DEFAULT,
            Ok(s("0001-01-01T00:00:00Z")),
        ),
        (
            TypeId::DATETIME,
            Some(vec![]),
            TypeId::INT,
            Err("Time.UnmarshalBinary: no data"),
        ),
        (
            TypeId::DATETIME,
            Some(bs_time(zero)),
            TypeId::INT,
            Ok(Value::Int(zero.unix())),
        ),
        (
            TypeId::DATETIME,
            Some(vec![]),
            TypeId::FLOAT,
            Err("Time.UnmarshalBinary: no data"),
        ),
        (
            TypeId::DATETIME,
            Some(bs_time(zero)),
            TypeId::FLOAT,
            Ok(Value::Float(zero.unix_nano() as f64 / 1e9)),
        ),
    ];
    for (from, data, to, want) in cases {
        let input = match data {
            Some(d) => Val::bytes(from, d),
            None => Val::nil(from),
        };
        let out = convert(&input, to);
        match want {
            Err(failure) => {
                let err = out.expect_err(failure).to_string();
                assert!(
                    err.contains(failure),
                    "{} to {}: {err} lacks {failure}",
                    from.name(),
                    to.name()
                );
            }
            Ok(v) => assert_eq!(out.unwrap(), Val::new(to, v)),
        }
    }
}

#[test]
fn test_convert_to_default() {
    let cases = [
        (TypeId::STRING, b"a".to_vec(), "a"),
        (TypeId::STRING, b"".to_vec(), ""),
        (TypeId::DEFAULT, b"abc".to_vec(), "abc"),
        (TypeId::BINARY, b"2016".to_vec(), "2016"),
        (TypeId::INT, bs_i64(3), "3"),
        (TypeId::FLOAT, bs_f64(-3.5), "-3.5"),
        (
            TypeId::DATETIME,
            bs_time(date(2006, 1, 2, 15, 4, 5, 0)),
            "2006-01-02T15:04:05Z",
        ),
        (
            TypeId::DATETIME,
            bs_time(DateTime::zero()),
            "0001-01-01T00:00:00Z",
        ),
    ];
    for (tid, data, want) in cases {
        assert_eq!(
            conv(tid, data, TypeId::DEFAULT),
            ok(TypeId::DEFAULT, s(want))
        );
    }
}

#[test]
fn test_convert_from_default() {
    let cases = [
        ("1", TypeId::INT, Value::Int(1)),
        ("1.3", TypeId::FLOAT, Value::Float(1.3)),
        ("true", TypeId::BOOL, Value::Bool(true)),
        ("2016", TypeId::BINARY, Value::Bytes(b"2016".to_vec())),
        ("", TypeId::BINARY, Value::Bytes(Vec::new())),
        ("hello", TypeId::STRING, s("hello")),
        ("", TypeId::STRING, s("")),
        (
            "2016",
            TypeId::DATETIME,
            Value::DateTime(date(2016, 1, 1, 0, 0, 0, 0)),
        ),
    ];
    for (input, to, want) in cases {
        assert_eq!(conv(TypeId::DEFAULT, input.into(), to), ok(to, want));
    }
}

#[test]
fn test_convert_to_binary() {
    let t = bs_time(date(2006, 1, 2, 15, 4, 5, 0));
    let cases = [
        (TypeId::STRING, b"a".to_vec(), b"a".to_vec()),
        (TypeId::STRING, b"".to_vec(), b"".to_vec()),
        (TypeId::DEFAULT, b"abc".to_vec(), b"abc".to_vec()),
        (TypeId::BINARY, b"2016".to_vec(), b"2016".to_vec()),
        (TypeId::INT, bs_i64(3), bs_i64(3)),
        (TypeId::FLOAT, bs_f64(-3.5), bs_f64(-3.5)),
        (TypeId::DATETIME, t.clone(), t),
    ];
    for (tid, data, want) in cases {
        assert_eq!(
            conv(tid, data, TypeId::BINARY),
            ok(TypeId::BINARY, Value::Bytes(want))
        );
    }
}

#[test]
fn test_convert_from_binary() {
    let cases = [
        (bs_bool(true), TypeId::BOOL, Value::Bool(true)),
        (bs_bool(false), TypeId::BOOL, Value::Bool(false)),
        (Vec::new(), TypeId::BOOL, Value::Bool(false)),
        (bs_i64(1), TypeId::INT, Value::Int(1)),
        (bs_f64(1.3), TypeId::FLOAT, Value::Float(1.3)),
        (
            b"2016".to_vec(),
            TypeId::BINARY,
            Value::Bytes(b"2016".to_vec()),
        ),
        (Vec::new(), TypeId::BINARY, Value::Bytes(Vec::new())),
        (b"hello".to_vec(), TypeId::STRING, s("hello")),
        (Vec::new(), TypeId::STRING, s("")),
        (
            bs_time(date(2016, 1, 1, 0, 0, 0, 0)),
            TypeId::DATETIME,
            Value::DateTime(date(2016, 1, 1, 0, 0, 0, 0)),
        ),
        (
            bs_time(DateTime::zero()),
            TypeId::DATETIME,
            Value::DateTime(DateTime::zero()),
        ),
        (
            float_array_as_bytes(&[1.0, 2.0]),
            TypeId::VFLOAT,
            Value::VFloat(vec![1.0, 2.0]),
        ),
    ];
    for (data, to, want) in cases {
        assert_eq!(conv(TypeId::BINARY, data, to), ok(to, want));
    }
}

#[test]
fn test_convert_string_to_date_time() {
    let cases = [
        ("2006-01-02T15:04:05", date(2006, 1, 2, 15, 4, 5, 0)),
        ("2006-01-02", date(2006, 1, 2, 0, 0, 0, 0)),
        ("2006-01", date(2006, 1, 1, 0, 0, 0, 0)),
        ("2006", date(2006, 1, 1, 0, 0, 0, 0)),
    ];
    for (input, want) in cases {
        assert_eq!(
            conv(TypeId::STRING, input.into(), TypeId::DATETIME),
            ok(TypeId::DATETIME, Value::DateTime(want))
        );
    }
}

#[test]
fn test_convert_date_time_to_string() {
    let cases = [
        (date(2006, 1, 2, 15, 4, 5, 0), "2006-01-02T15:04:05Z"),
        (date(2006, 1, 2, 0, 0, 0, 0), "2006-01-02T00:00:00Z"),
        (date(2006, 1, 1, 0, 0, 0, 0), "2006-01-01T00:00:00Z"),
    ];
    for (t, want) in cases {
        assert_eq!(
            conv(TypeId::DATETIME, bs_time(t), TypeId::STRING),
            ok(TypeId::STRING, s(want))
        );
    }
}

#[test]
fn test_convert_from_password() {
    let cases: [(&str, TypeId, Result<Value, &str>); 8] = [
        ("", TypeId::STRING, Ok(s(""))),
        ("password", TypeId::PASSWORD, Ok(s("password"))),
        ("password", TypeId::STRING, Ok(s("password"))),
        (
            "password",
            TypeId::BINARY,
            Ok(Value::Bytes(b"password".to_vec())),
        ),
        (
            "password",
            TypeId::DEFAULT,
            Err("Cannot convert password to type default"),
        ),
        (
            "password",
            TypeId::INT,
            Err("Cannot convert password to type int"),
        ),
        (
            "password",
            TypeId::FLOAT,
            Err("Cannot convert password to type float"),
        ),
        (
            "password",
            TypeId::BOOL,
            Err("Cannot convert password to type bool"),
        ),
    ];
    for (input, to, want) in cases {
        let out = conv(TypeId::PASSWORD, input.into(), to);
        match want {
            Ok(v) => assert_eq!(out, ok(to, v)),
            Err(e) => assert_eq!(out, Err(e.to_string())),
        }
    }
}

#[test]
fn test_convert_to_password() {
    let cases: [(TypeId, Vec<u8>, Result<&str, &str>); 7] = [
        (TypeId::STRING, b"testing".to_vec(), Ok("$2a$10$")),
        (TypeId::PASSWORD, b"testing".to_vec(), Ok("testing")),
        (TypeId::DEFAULT, b"testing".to_vec(), Ok("$2a$10$")),
        (
            TypeId::STRING,
            b"".to_vec(),
            Err("Password too short, i.e. should have at least 6 chars"),
        ),
        (
            TypeId::INT,
            bs_i64(1),
            Err("Cannot convert int to type password"),
        ),
        (
            TypeId::FLOAT,
            bs_f64(1.0),
            Err("Cannot convert float to type password"),
        ),
        (
            TypeId::BOOL,
            bs_bool(true),
            Err("Cannot convert bool to type password"),
        ),
    ];
    for (tid, data, want) in cases {
        let plain = data.clone();
        let out = conv(tid, data, TypeId::PASSWORD);
        match want {
            Err(e) => assert_eq!(out, Err(e.to_string())),
            Ok(w) if tid == TypeId::PASSWORD => assert_eq!(out, ok(TypeId::PASSWORD, s(w))),
            Ok(_) => {
                let out = out.unwrap();
                assert_eq!(out.tid(), TypeId::PASSWORD);
                let Some(Value::Str(hash)) = out.value() else {
                    panic!("no hash")
                };
                verify_password(&plain, hash).unwrap();
            }
        }
    }
}

#[test]
fn test_same_conversion_bool() {
    for b in [true, false] {
        assert_eq!(
            conv(TypeId::BINARY, bs_bool(b), TypeId::BOOL),
            ok(TypeId::BOOL, Value::Bool(b))
        );
    }
}

#[test]
fn test_convert_int_to_bool() {
    for (i, want) in [(3, true), (-3, true), (0, false)] {
        assert_eq!(
            conv(TypeId::INT, bs_i64(i), TypeId::BOOL),
            ok(TypeId::BOOL, Value::Bool(want))
        );
    }
}

#[test]
fn test_convert_float_to_bool() {
    let cases = [
        (3.0, true),
        (-3.5, true),
        (0.0, false),
        (f64::NAN, true),
        (f64::INFINITY, true),
        (f64::NEG_INFINITY, true),
    ];
    for (f, want) in cases {
        assert_eq!(
            conv(TypeId::FLOAT, bs_f64(f), TypeId::BOOL),
            ok(TypeId::BOOL, Value::Bool(want))
        );
    }
}

#[test]
fn test_convert_string_to_bool() {
    let cases: [(&str, Result<bool, &str>); 12] = [
        ("1", Ok(true)),
        ("true", Ok(true)),
        ("True", Ok(true)),
        ("T", Ok(true)),
        ("F", Ok(false)),
        ("0", Ok(false)),
        ("false", Ok(false)),
        ("False", Ok(false)),
        ("", Err(r#"strconv.ParseBool: parsing "": invalid syntax"#)),
        (
            "srfrog",
            Err(r#"strconv.ParseBool: parsing "srfrog": invalid syntax"#),
        ),
        (
            "3",
            Err(r#"strconv.ParseBool: parsing "3": invalid syntax"#),
        ),
        (
            "-3",
            Err(r#"strconv.ParseBool: parsing "-3": invalid syntax"#),
        ),
    ];
    for (input, want) in cases {
        let out = conv(TypeId::STRING, input.into(), TypeId::BOOL);
        match want {
            Ok(b) => assert_eq!(out, ok(TypeId::BOOL, Value::Bool(b))),
            Err(e) => assert_eq!(out, Err(e.to_string())),
        }
    }
}

#[test]
fn test_convert_date_time_to_bool() {
    let now = DateTime::from_unix(1_700_000_000, 5);
    assert_eq!(
        conv(TypeId::DATETIME, bs_time(now), TypeId::BOOL),
        Err("Cannot convert datetime to type bool".into())
    );
}

#[test]
fn test_convert_bool_to_date_time() {
    assert_eq!(
        conv(TypeId::BOOL, bs_bool(true), TypeId::DATETIME),
        Err("Cannot convert bool to type datetime".into())
    );
}

#[test]
fn test_convert_bool_to_int() {
    for (b, want) in [(true, 1), (false, 0)] {
        assert_eq!(
            conv(TypeId::BOOL, bs_bool(b), TypeId::INT),
            ok(TypeId::INT, Value::Int(want))
        );
    }
}

#[test]
fn test_truthy() {
    let cases = [
        (TypeId::STRING, b"true".to_vec()),
        (TypeId::DEFAULT, b"true".to_vec()),
        (TypeId::INT, bs_i64(1)),
        (TypeId::INT, bs_i64(-1)),
        (TypeId::FLOAT, bs_f64(1.0)),
        (TypeId::FLOAT, bs_f64(-1.0)),
    ];
    for (tid, data) in cases {
        assert_eq!(
            conv(tid, data, TypeId::BOOL).unwrap().value(),
            Some(&Value::Bool(true))
        );
    }
}

#[test]
fn test_falsy() {
    let cases = [
        (TypeId::STRING, b"false".to_vec()),
        (TypeId::DEFAULT, b"false".to_vec()),
        (TypeId::INT, bs_i64(0)),
        (TypeId::FLOAT, bs_f64(0.0)),
        (TypeId::BINARY, Vec::new()),
    ];
    for (tid, data) in cases {
        assert_eq!(
            conv(tid, data, TypeId::BOOL).unwrap().value(),
            Some(&Value::Bool(false))
        );
    }
}

#[test]
fn test_same_conversion_float() {
    for f in [3.4434, -3.0, 0.5e2] {
        assert_eq!(
            conv(TypeId::BINARY, bs_f64(f), TypeId::FLOAT),
            ok(TypeId::FLOAT, Value::Float(f))
        );
    }
}

#[test]
fn test_same_conversion_int() {
    for i in [3, -3, 9999, 0] {
        assert_eq!(
            conv(TypeId::BINARY, bs_i64(i), TypeId::INT),
            ok(TypeId::INT, Value::Int(i))
        );
    }
}

#[test]
fn test_convert_bool_to_float() {
    for (b, want) in [(true, 1.0), (false, 0.0)] {
        assert_eq!(
            conv(TypeId::BOOL, bs_bool(b), TypeId::FLOAT),
            ok(TypeId::FLOAT, Value::Float(want))
        );
    }
}

#[test]
fn test_convert_bool_to_string() {
    for (b, want) in [(true, "true"), (false, "false")] {
        assert_eq!(
            conv(TypeId::BOOL, bs_bool(b), TypeId::STRING),
            ok(TypeId::STRING, s(want))
        );
    }
}

#[test]
fn test_convert_int_to_float() {
    for (i, want) in [(3, 3.0), (-3, -3.0), (0, 0.0)] {
        assert_eq!(
            conv(TypeId::INT, bs_i64(i), TypeId::FLOAT),
            ok(TypeId::FLOAT, Value::Float(want))
        );
    }
}

#[test]
fn test_convert_float_to_int() {
    let cases: [(f64, Result<i64, &str>); 8] = [
        (3.0, Ok(3)),
        (-3.0, Ok(-3)),
        (0.0, Ok(0)),
        (522638295213.3243, Ok(522638295213)),
        (-522638295213.3243, Ok(-522638295213)),
        (f64::NAN, Err("Float out of int64 range")),
        (f64::INFINITY, Err("Float out of int64 range")),
        (f64::NEG_INFINITY, Err("Float out of int64 range")),
    ];
    for (f, want) in cases {
        let out = conv(TypeId::FLOAT, bs_f64(f), TypeId::INT);
        match want {
            Ok(i) => assert_eq!(out, ok(TypeId::INT, Value::Int(i))),
            Err(e) => assert_eq!(out, Err(e.to_string())),
        }
    }
}

#[test]
fn test_convert_float_to_int_at_the_int64_bounds() {
    // Dgraph lets 2^63 through, which converts to MinInt64 on amd64 and MaxInt64 on arm64 (ledger 10).
    let cases: [(f64, Result<i64, &str>); 4] = [
        (9_223_372_036_854_775_808.0, Err("Float out of int64 range")),
        (9_223_372_036_854_774_784.0, Ok(9_223_372_036_854_774_784)),
        (-9_223_372_036_854_775_808.0, Ok(i64::MIN)),
        (
            -9_223_372_036_854_777_856.0,
            Err("Float out of int64 range"),
        ),
    ];
    for (f, want) in cases {
        let out = conv(TypeId::FLOAT, bs_f64(f), TypeId::INT);
        match want {
            Ok(i) => assert_eq!(out, ok(TypeId::INT, Value::Int(i))),
            Err(e) => assert_eq!(out, Err(e.to_string())),
        }
    }
}

#[test]
fn test_convert_string_to_int() {
    let cases: [(&str, Result<i64, &str>); 9] = [
        ("1", Ok(1)),
        ("13816", Ok(13816)),
        ("-1221", Ok(-1221)),
        ("0", Ok(0)),
        ("203716381366627", Ok(203716381366627)),
        ("", Err(r#"strconv.ParseInt: parsing "": invalid syntax"#)),
        (
            "srfrog",
            Err(r#"strconv.ParseInt: parsing "srfrog": invalid syntax"#),
        ),
        (
            "3.0",
            Err(r#"strconv.ParseInt: parsing "3.0": invalid syntax"#),
        ),
        (
            "-3a.5",
            Err(r#"strconv.ParseInt: parsing "-3a.5": invalid syntax"#),
        ),
    ];
    for (input, want) in cases {
        let out = conv(TypeId::STRING, input.into(), TypeId::INT);
        match want {
            Ok(i) => assert_eq!(out, ok(TypeId::INT, Value::Int(i))),
            Err(e) => assert_eq!(out, Err(e.to_string())),
        }
    }
}

#[test]
fn test_convert_string_to_float() {
    let cases: [(&str, Result<f64, &str>); 10] = [
        ("1", Ok(1.0)),
        ("13816.251", Ok(13816.251)),
        ("-1221.12", Ok(-1221.12)),
        ("-0.0", Ok(-0.0)),
        ("1e10", Ok(1e10)),
        ("1e-2", Ok(0.01)),
        ("", Err(r#"strconv.ParseFloat: parsing "": invalid syntax"#)),
        (
            "srfrog",
            Err(r#"strconv.ParseFloat: parsing "srfrog": invalid syntax"#),
        ),
        (
            "-3a.5",
            Err(r#"strconv.ParseFloat: parsing "-3a.5": invalid syntax"#),
        ),
        (
            "1e400",
            Err(r#"strconv.ParseFloat: parsing "1e400": value out of range"#),
        ),
    ];
    for (input, want) in cases {
        let out = conv(TypeId::STRING, input.into(), TypeId::FLOAT);
        match want {
            Ok(f) => assert_eq!(out, ok(TypeId::FLOAT, Value::Float(f))),
            Err(e) => assert_eq!(out, Err(e.to_string())),
        }
    }
}

#[test]
fn test_convert_float_to_date_time() {
    let cases = [
        (1257894000.0, date(2009, 11, 10, 23, 0, 0, 0)),
        (-4410000.0, date(1969, 11, 10, 23, 0, 0, 0)),
        (2204578800.0, date(2039, 11, 10, 23, 0, 0, 0)),
        (-2150326800.0, date(1901, 11, 10, 23, 0, 0, 0)),
        (1257894000.001, date(2009, 11, 10, 23, 0, 0, 999927)),
        (-4409999.999, date(1969, 11, 10, 23, 0, 0, 1000001)),
    ];
    for (f, want) in cases {
        assert_eq!(
            conv(TypeId::FLOAT, bs_f64(f), TypeId::DATETIME),
            ok(TypeId::DATETIME, Value::DateTime(want))
        );
    }
}

#[test]
fn test_convert_float_to_date_time_outside_int64() {
    for f in [
        1e19,
        -1e19,
        9_223_372_036_854_775_808.0,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
    ] {
        assert_eq!(
            conv(TypeId::FLOAT, bs_f64(f), TypeId::DATETIME),
            Err("Float out of int64 range".into()),
            "{f}"
        );
    }
}

#[test]
fn test_convert_big_float_to_date_time_outside_int64() {
    for text in [
        "1e19",
        "9223372036854775808",
        "-9223372036854775809",
        "+Inf",
        "-Inf",
    ] {
        assert_eq!(
            conv(TypeId::BIGFLOAT, text.into(), TypeId::DATETIME),
            Err("Float out of int64 range".into()),
            "{text}"
        );
    }
    for text in ["9223372036854775807.5", "-9223372036854775808", "1.5"] {
        assert!(
            conv(TypeId::BIGFLOAT, text.into(), TypeId::DATETIME).is_ok(),
            "{text}"
        );
    }
}

#[test]
fn test_convert_invalid_utf8_to_string_is_refused() {
    let invalid = vec![0xe6, 0x97, 0xff];
    let refused = |name: &str| Err(format!("Value is not valid UTF-8 for type {name}"));
    for (from, to, name) in [
        (TypeId::BINARY, TypeId::STRING, "string"),
        (TypeId::BINARY, TypeId::DEFAULT, "string"),
        (TypeId::STRING, TypeId::STRING, "string"),
        (TypeId::DEFAULT, TypeId::DEFAULT, "string"),
        (TypeId::PASSWORD, TypeId::STRING, "string"),
        (TypeId::BINARY, TypeId::PASSWORD, "password"),
        (TypeId::PASSWORD, TypeId::PASSWORD, "password"),
    ] {
        assert_eq!(
            conv(from, invalid.clone(), to),
            refused(name),
            "{from:?} to {to:?}"
        );
    }
    // Binary keeps the bytes, and valid UTF-8 converts.
    assert_eq!(
        conv(TypeId::STRING, invalid.clone(), TypeId::BINARY),
        ok(TypeId::BINARY, Value::Bytes(invalid.clone()))
    );
    assert_eq!(
        conv(TypeId::BINARY, "日本".into(), TypeId::STRING),
        ok(TypeId::STRING, s("日本"))
    );
    let bin = Val::new(TypeId::BINARY, Value::Bytes(invalid));
    assert_eq!(
        marshal(&bin, TypeId::STRING).map_err(|e| e.to_string()),
        refused("string")
    );
}

#[test]
fn test_marshal_binary() {
    use ultragraphdb_core_types::marshal_binary;
    assert_eq!(
        marshal_binary(&Val::new(TypeId::INT, Value::Int(-2))).unwrap(),
        bs_i64(-2)
    );
    assert_eq!(
        marshal_binary(&Val::new(TypeId::STRING, s("ab"))).unwrap(),
        b"ab".to_vec()
    );
    // Go's ValueForType of binary holds a *[]byte, on which toBinary's assertion panics.
    assert_eq!(
        marshal_binary(&Val::nil(TypeId::GEO))
            .unwrap_err()
            .to_string(),
        "interface conversion: interface {} is *[]uint8, not []uint8"
    );
    assert_eq!(
        marshal_binary(&Val::new(TypeId::INT, Value::Bool(true)))
            .unwrap_err()
            .to_string(),
        "interface conversion: interface {} is bool, not int64"
    );
}

#[test]
fn test_convert_date_time_to_float() {
    let cases = [
        (date(2009, 11, 10, 23, 0, 0, 0), 1257894000.0),
        (date(1969, 11, 10, 23, 0, 0, 0), -4410000.0),
        (date(2039, 11, 10, 23, 0, 0, 0), 2204578800.0),
        (date(1901, 11, 10, 23, 0, 0, 0), -2150326800.0),
        (date(2009, 11, 10, 23, 0, 0, 1000000), 1257894000.001),
        (date(1969, 11, 10, 23, 0, 0, 1000000), -4409999.999),
    ];
    for (t, want) in cases {
        assert_eq!(
            conv(TypeId::DATETIME, bs_time(t), TypeId::FLOAT),
            ok(TypeId::FLOAT, Value::Float(want))
        );
    }
}

#[test]
fn test_convert_int_to_date_time() {
    for (i, want) in [
        (1257894000, date(2009, 11, 10, 23, 0, 0, 0)),
        (-4410000, date(1969, 11, 10, 23, 0, 0, 0)),
    ] {
        assert_eq!(
            conv(TypeId::INT, bs_i64(i), TypeId::DATETIME),
            ok(TypeId::DATETIME, Value::DateTime(want))
        );
    }
}

#[test]
fn test_convert_date_time_to_int() {
    let cases = [
        (date(2009, 11, 10, 23, 0, 0, 0), 1257894000),
        (date(1969, 11, 10, 23, 0, 0, 0), -4410000),
        (date(2039, 11, 10, 23, 0, 0, 0), 2204578800),
        (date(1901, 11, 10, 23, 0, 0, 0), -2150326800),
    ];
    for (t, want) in cases {
        assert_eq!(
            conv(TypeId::DATETIME, bs_time(t), TypeId::INT),
            ok(TypeId::INT, Value::Int(want))
        );
    }
}

#[test]
fn test_convert_to_string() {
    let cases = [
        (TypeId::FLOAT, bs_f64(13816.251), "13816.251"),
        (TypeId::INT, bs_i64(-1221), "-1221"),
        (TypeId::BOOL, bs_bool(true), "true"),
        (TypeId::STRING, b"srfrog".to_vec(), "srfrog"),
        (TypeId::PASSWORD, b"password".to_vec(), "password"),
        (
            TypeId::DATETIME,
            bs_time(date(2006, 1, 2, 15, 4, 5, 0)),
            "2006-01-02T15:04:05Z",
        ),
    ];
    for (tid, data, want) in cases {
        assert_eq!(conv(tid, data, TypeId::STRING), ok(TypeId::STRING, s(want)));
    }
}

#[test]
fn test_fixed_zone_round_trip() {
    let t = DateTime::new(2018, 5, 30, 9, 30, 10, 0, Zone::Fixed(-6 * 3600));
    let back = conv(TypeId::BINARY, bs_time(t), TypeId::DATETIME).unwrap();
    assert_eq!(back.value(), Some(&Value::DateTime(t)));
}

// ---------- Golden conversion matrix ----------

#[derive(Debug, Deserialize)]
struct Rendered {
    go_type: String,
    #[serde(default)]
    nil: bool,
    #[serde(default)]
    bin: String,
    #[serde(default)]
    bin_err: String,
    #[serde(default)]
    str: String,
    #[serde(default)]
    str_err: String,
    #[serde(default)]
    prec: u32,
    #[serde(default)]
    p: String,
    #[serde(default)]
    salted: bool,
    #[serde(default)]
    panic: String,
}

#[derive(Debug, Deserialize)]
struct ConvCase {
    from: i32,
    to: i32,
    #[serde(rename = "in")]
    input: String,
    #[serde(default)]
    err: String,
    #[serde(default)]
    panic: String,
    out: Option<Rendered>,
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn variant_of(go_type: &str) -> &'static str {
    match go_type {
        "[]uint8" => "Bytes",
        "string" => "Str",
        "int64" => "Int",
        "float64" => "Float",
        "bool" => "Bool",
        "time.Time" => "DateTime",
        "big.Float" | "*big.Float" => "BigFloat",
        "[]float32" => "VFloat",
        "<nil>" => "nil",
        t if t.starts_with("*geom.") => "Geo",
        t => panic!("unexpected Go type {t}"),
    }
}

fn variant(v: Option<&Value>) -> &'static str {
    match v {
        None => "nil",
        Some(Value::Bytes(_)) => "Bytes",
        Some(Value::Str(_)) => "Str",
        Some(Value::Int(_)) => "Int",
        Some(Value::Float(_)) => "Float",
        Some(Value::Bool(_)) => "Bool",
        Some(Value::DateTime(_)) => "DateTime",
        Some(Value::Geo(_)) => "Geo",
        Some(Value::Uid(_)) => "Uid",
        Some(Value::BigFloat(_)) => "BigFloat",
        Some(Value::VFloat(_)) => "VFloat",
    }
}

fn marshal_bytes(v: &Val, to: TypeId) -> Result<Vec<u8>, String> {
    match marshal(v, to).map_err(|e| e.to_string())?.into_value() {
        Some(Value::Bytes(b)) => Ok(b),
        Some(Value::Str(s)) => Ok(s.into_bytes()),
        None => Err("<nil>".into()),
        Some(other) => panic!("marshal gave {other:?}"),
    }
}

/// A port result that differs from Go's.
#[derive(Debug)]
enum Expected {
    /// The conversion fails with this text.
    Err(&'static str),
    /// The conversion succeeds, and the value marshals to binary and to string as these texts.
    Marshals {
        bin: &'static str,
        str: &'static str,
    },
}

/// The golden conversions on which the port differs from Go: the case, as `from -> to of input`, the ledger entry of
/// `openspec/notes/dgraph-bugs-found-in-port.md` (`None` where Go panics, which the port does not), and the port's
/// result.
const DEVIATIONS: &[(&str, Option<u32>, Expected)] = &[
    // Go panics on SetFloat64(NaN); the port returns the panic text as its error.
    (
        "3 -> 11 of 010000000000f87f",
        None,
        Expected::Err("Float.SetFloat64(NaN)"),
    ),
    // Go converts to a *big.Float and then panics asserting a big.Float in Marshal; the port marshals the value.
    (
        "4 -> 11 of 00",
        None,
        Expected::Marshals { bin: "0", str: "0" },
    ),
    (
        "4 -> 11 of 01",
        None,
        Expected::Marshals { bin: "1", str: "1" },
    ),
    (
        "4 -> 11 of 0105",
        None,
        Expected::Marshals { bin: "1", str: "1" },
    ),
    (
        "5 -> 11 of 01000000000000000000000000ffff",
        None,
        Expected::Marshals {
            bin: "-6.7953645788713455e+09",
            str: "-6795364579",
        },
    ),
    (
        "5 -> 11 of 010000000ebb4b37e500000000ffff",
        None,
        Expected::Marshals {
            bin: "1.136214245e+09",
            str: "1136214245",
        },
    ),
    (
        "5 -> 11 of 010000000ece17b88000000000ffff",
        None,
        Expected::Marshals {
            bin: "1.4516064e+09",
            str: "1451606400",
        },
    ),
    (
        "5 -> 11 of 010000000ec28be770000f4240ffff",
        None,
        Expected::Marshals {
            bin: "1.257894000001e+09",
            str: "1257894000",
        },
    ),
    (
        "5 -> 11 of 010000000e774eac70000f4240ffff",
        None,
        Expected::Marshals {
            bin: "-4.409999999e+06",
            str: "-4409999.999",
        },
    ),
    (
        "5 -> 11 of 010000000df76694f000000000ffff",
        None,
        Expected::Marshals {
            bin: "-2.1503268e+09",
            str: "-2150326800",
        },
    ),
    (
        "5 -> 11 of 010000000ed2a0ba821dcd6500fe98",
        None,
        Expected::Marshals {
            bin: "1.5276942105e+09",
            str: "1527694210",
        },
    ),
    (
        "5 -> 11 of 010000000ed29cbaf100000000059f",
        None,
        Expected::Marshals {
            bin: "1.527432177e+09",
            str: "1527432177",
        },
    ),
    (
        "5 -> 11 of 020000000ed29dbece075bcd15014a0f",
        None,
        Expected::Marshals {
            bin: "1.5274987021234567e+09",
            str: "1527498702",
        },
    ),
    (
        "5 -> 11 of 010000000ed29e0c35000000000000",
        None,
        Expected::Marshals {
            bin: "1.527518517e+09",
            str: "1527518517",
        },
    ),
    (
        "5 -> 11 of 010000000ed29caca50000000005dc",
        None,
        Expected::Marshals {
            bin: "1.527428517e+09",
            str: "1527428517",
        },
    ),
    (
        "5 -> 11 of 01000000497786387f3b9ac9ffffff",
        None,
        Expected::Marshals {
            bin: "-4.8521162319337225e+09",
            str: "-4852116232",
        },
    ),
    (
        "5 -> 11 of 01000000497786388000000000ffff",
        None,
        Expected::Marshals {
            bin: "-4.8521162319337225e+09",
            str: "-4852116232",
        },
    ),
    (
        "5 -> 11 of 01fffffffffc3c478000000000ffff",
        None,
        Expected::Marshals {
            bin: "-6.8585229788713455e+09",
            str: "-6858522979",
        },
    ),
    (
        "5 -> 11 of 01000000109d53740432f2d800ffff",
        None,
        Expected::Marshals {
            bin: "-9.223372036854776e+09",
            str: "-9223372037",
        },
    ),
    (
        "5 -> 11 of 010000000c51d079fb08a7f1ffffff",
        None,
        Expected::Marshals {
            bin: "9.223372036854776e+09",
            str: "9223372037",
        },
    ),
    (
        "5 -> 11 of 010000000eb04dabc000000007ffff",
        None,
        Expected::Marshals {
            bin: "9.518256e+08",
            str: "951825600",
        },
    ),
    // Go converts a float outside int64 as its platform does; the port refuses it.
    (
        "3 -> 5 of 408cb5781daf1544",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of 408cb5781daf15c4",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of 50efe2d6e41a4b44",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 2 of 000000000000e043",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of 000000000000e043",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of ffffffffffffef7f",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of 010000000000f87f",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of 000000000000f07f",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of 000000000000f0ff",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of f3ff4d451e429e75",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of 031e4b99bf11ae74",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of 9c8428183c3faed4",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of 322668df4d6b156d",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of cf81def8e1fa90f0",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of f49bd58cf46e9ff8",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of 9001f27bfb101fef",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of 9df221c8196421c8",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of 0acce81aa5efa2fc",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of f18158dc7a20eb55",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of 6dfd1a0e69fce06f",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of f8dd5ae781eb6575",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of 7067318d759b494a",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of 0cc4e40fc70e57f1",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of 9b3c7a288b3ae97a",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of 0856421604d9b7fb",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of 8ad6b20bea64787a",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of f99a4e5c1f3e256e",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of 9fa1d6fc01b89478",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of a302b0eab4088deb",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of c224a2f6fa547e64",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "3 -> 5 of fd570ecd72c329cf",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "11 -> 5 of 31653231",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "11 -> 5 of 3132333435363738393031323334353637383930313233343536373839302e313233",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "11 -> 5 of 392e33653138",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "11 -> 5 of 2d392e33653138",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "11 -> 5 of 2b496e66",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "11 -> 5 of 2d496e66",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
    (
        "11 -> 5 of 3165343030",
        Some(10),
        Expected::Err("Float out of int64 range"),
    ),
];

/// The golden conversions whose Go string holds bytes that are not UTF-8: a string value holds valid UTF-8, so the
/// port refuses each (the user's Q4, ledger 17, 18 and 31). Each is checked to hold such bytes in Go and to fail with
/// [`not_utf8`] in the port: 159 binary inputs each converted to default, password and string (477), the string form
/// of 316 binary results, and 4 default and string inputs converted to default and string.
const NOT_UTF8_CASES: usize = 797;

/// The port's refusal of bytes that are not UTF-8 as a value of type `to`.
fn not_utf8(to: i32) -> String {
    let name = if to == TypeId::PASSWORD.enum_value() {
        "password"
    } else {
        "string"
    };
    format!("Value is not valid UTF-8 for type {name}")
}

fn is_utf8(hex: &str) -> bool {
    std::str::from_utf8(&unhex(hex)).is_ok()
}

/// How a golden case compares.
#[derive(Debug, PartialEq)]
enum Outcome {
    /// The port gives Go's result.
    Equal,
    /// The port gives the result its `DEVIATIONS` entry names.
    Deviation,
    /// Go's string holds bytes that are not UTF-8, and the port refuses them.
    NotUtf8,
}

fn check_rendered(
    c: &ConvCase,
    input_plain: &[u8],
    out: &Val,
    want: &Rendered,
    deviation: Option<&Expected>,
) -> Result<Outcome, String> {
    let ctx = format!("{} -> {} of {}", c.from, c.to, c.input);
    if variant(out.value()) != variant_of(&want.go_type) && !want.nil {
        return Err(format!(
            "{ctx}: value {:?}, Go type {}",
            out.value(),
            want.go_type
        ));
    }
    if want.nil {
        return if out.value().is_none() {
            Ok(Outcome::Equal)
        } else {
            Err(format!("{ctx}: want nil, got {:?}", out.value()))
        };
    }
    if let Some(Value::BigFloat(b)) = out.value()
        && (b.prec() != want.prec || b.text_p() != want.p)
    {
        return Err(format!(
            "{ctx}: bigfloat {} at {}, Go {} at {}",
            b.text_p(),
            b.prec(),
            want.p,
            want.prec
        ));
    }
    if want.salted {
        let Some(Value::Str(h)) = out.value() else {
            return Err(format!("{ctx}: no hash"));
        };
        if !h.starts_with("$2a$") {
            return Err(format!("{ctx}: hash {h:?}"));
        }
        if c.from == TypeId::PASSWORD.enum_value() {
            // A password value converted to itself is the stored hash, unchanged.
            return if h.as_bytes().get(..7) == Some(&unhex(&want.str)[..]) {
                Ok(Outcome::Equal)
            } else {
                Err(format!("{ctx}: {h:?}"))
            };
        }
        return verify_password(input_plain, h)
            .map(|()| Outcome::Equal)
            .map_err(|e| format!("{ctx}: {e}"));
    }
    let bin = marshal_bytes(out, TypeId::BINARY);
    let st = marshal_bytes(out, TypeId::STRING);
    if !want.panic.is_empty() {
        // Go panics in Marshal; the port marshals the value.
        let Some(Expected::Marshals { bin: b, str: s }) = deviation else {
            return Err(format!(
                "{ctx}: Go panics ({}), no deviation listed",
                want.panic
            ));
        };
        return if bin.as_deref() == Ok(b.as_bytes()) && st.as_deref() == Ok(s.as_bytes()) {
            Ok(Outcome::Deviation)
        } else {
            Err(format!(
                "{ctx}: binary {:?} and string {:?}, listed {b:?} and {s:?}",
                bin.map(|b| String::from_utf8_lossy(&b).into_owned()),
                st.map(|b| String::from_utf8_lossy(&b).into_owned())
            ))
        };
    }
    if deviation.is_some() {
        return Err(format!(
            "{ctx}: listed as a deviation, but the port converts it as Go does"
        ));
    }
    match &bin {
        Ok(b) if want.bin_err.is_empty() && *b == unhex(&want.bin) => {}
        Err(e) if !want.bin_err.is_empty() && *e == want.bin_err => {}
        _ => {
            return Err(format!(
                "{ctx}: binary {bin:?}, Go {} / {}",
                want.bin, want.bin_err
            ));
        }
    }
    match &st {
        Ok(b) if want.str_err.is_empty() && *b == unhex(&want.str) => Ok(Outcome::Equal),
        Err(e) if !want.str_err.is_empty() && *e == want.str_err => Ok(Outcome::Equal),
        Err(e) if want.str_err.is_empty() && !is_utf8(&want.str) && *e == not_utf8(9) => {
            Ok(Outcome::NotUtf8)
        }
        _ => Err(format!(
            "{ctx}: string {:?}, Go {:?} / {}",
            st.as_ref().map(|b| String::from_utf8_lossy(b).into_owned()),
            String::from_utf8_lossy(&unhex(&want.str)),
            want.str_err
        )),
    }
}

#[test]
fn test_golden_conversion_matrix() {
    let path = ultragraphdb_tool_bdd::testdata_dir().join("golden/types_convert.json");
    let golden = ultragraphdb_tool_bdd::load_golden::<ConvCase>(&path);
    let mut failures = Vec::new();
    let (mut deviations, mut not_utf8_cases) = (0, 0);
    for c in golden.cases() {
        let (input, plain) = match c.input.strip_prefix("string:") {
            Some(v) => (
                Val::new(TypeId::new(c.from), Value::str(v)),
                v.as_bytes().to_vec(),
            ),
            None => {
                let b = unhex(&c.input);
                (Val::bytes(TypeId::new(c.from), b.clone()), b)
            }
        };
        let name = format!("{} -> {} of {}", c.from, c.to, c.input);
        let deviation = DEVIATIONS
            .iter()
            .find(|(n, _, _)| *n == name)
            .map(|(_, _, e)| e);
        let out = convert(&input, TypeId::new(c.to));
        let res = match (&out, &c.out, deviation) {
            (Err(e), None, None) if !c.err.is_empty() && e.to_string() == c.err => {
                Ok(Outcome::Equal)
            }
            // Go panics converting; the port fails with the listed text.
            (Err(e), None, Some(Expected::Err(t)))
                if !c.panic.is_empty() && e.to_string() == *t =>
            {
                Ok(Outcome::Deviation)
            }
            // Go converts; the port refuses with the listed text.
            (Err(e), Some(_), Some(Expected::Err(t))) if e.to_string() == *t => {
                Ok(Outcome::Deviation)
            }
            (Err(e), Some(want), None)
                if !is_utf8(&want.str)
                    && want.str_err.is_empty()
                    && e.to_string() == not_utf8(c.to) =>
            {
                Ok(Outcome::NotUtf8)
            }
            (Ok(v), Some(want), _) => check_rendered(c, &plain, v, want, deviation),
            _ => Err(format!(
                "{name}: {out:?}, Go err {:?} panic {:?}, listed {deviation:?}",
                c.err, c.panic
            )),
        };
        match res {
            Ok(Outcome::Equal) => {}
            Ok(Outcome::Deviation) => deviations += 1,
            Ok(Outcome::NotUtf8) => not_utf8_cases += 1,
            Err(e) => failures.push(e),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        golden.cases().len(),
        failures.join("\n")
    );
    assert_eq!(
        deviations,
        DEVIATIONS.len(),
        "every deviation names a golden case"
    );
    assert_eq!(not_utf8_cases, NOT_UTF8_CASES, "cases refused as not UTF-8");
}

#[test]
fn test_marshal_json() {
    use ultragraphdb_core_types::{BigFloat, Geometry, Layout, marshal_json};
    let j = |v: Val| String::from_utf8(marshal_json(&v).unwrap()).unwrap();
    assert_eq!(j(Val::new(TypeId::INT, Value::Int(-3))), "-3");
    assert_eq!(j(Val::new(TypeId::FLOAT, Value::Float(1e21))), "1e+21");
    assert_eq!(j(Val::new(TypeId::FLOAT, Value::Float(0.5))), "0.5");
    assert_eq!(j(Val::new(TypeId::BOOL, Value::Bool(true))), "true");
    assert_eq!(
        j(Val::new(TypeId::STRING, Value::str("a<b\"\n"))),
        r#""a\u003cb\"\n""#
    );
    assert_eq!(j(Val::nil(TypeId::DEFAULT)), r#""""#);
    assert_eq!(
        j(Val::new(
            TypeId::DATETIME,
            Value::DateTime(DateTime::utc(2006, 1, 2, 15, 4, 5, 0))
        )),
        r#""2006-01-02T15:04:05Z""#
    );
    let b = BigFloat::unmarshal_text("1.5", 200).unwrap();
    assert_eq!(j(Val::new(TypeId::BIGFLOAT, Value::BigFloat(b))), "1.5");
    let p = Geometry::point(Layout::XY, &[1.0, 2.0]).unwrap();
    assert_eq!(
        j(Val::new(TypeId::GEO, Value::Geo(p))),
        r#"{"type":"Point","coordinates":[1,2]}"#
    );
    assert_eq!(
        marshal_json(&Val::new(TypeId::FLOAT, Value::Float(f64::NAN)))
            .unwrap_err()
            .to_string(),
        "json: unsupported value: NaN"
    );
    assert_eq!(
        marshal_json(&Val::nil(TypeId::UID))
            .unwrap_err()
            .to_string(),
        "invalid type for MarshalJSON: 7"
    );
}

/// Go v25.5.0's `Val.MarshalJSON` of a nil value, as Go 1.27 answers `Val{Tid: id}.MarshalJSON()`. Deviation: Go
/// panics on the type assertion of every type whose arm asserts `v.Value`; the port returns the panic's text. A
/// string or default value reads its zero value through `Safe`, and every other type is Go's refusal.
#[test]
fn test_marshal_json_nil() {
    use ultragraphdb_core_types::marshal_json;
    let cases: [(TypeId, Result<&str, &str>); 16] = [
        (TypeId::DEFAULT, Ok(r#""""#)),
        (TypeId::BINARY, Err("invalid type for MarshalJSON: 1")),
        (
            TypeId::INT,
            Err("interface conversion: interface {} is nil, not int64"),
        ),
        (
            TypeId::FLOAT,
            Err("interface conversion: interface {} is nil, not float64"),
        ),
        (
            TypeId::BOOL,
            Err("interface conversion: interface {} is nil, not bool"),
        ),
        (
            TypeId::DATETIME,
            Err("interface conversion: interface {} is nil, not time.Time"),
        ),
        // geom.T is an interface type, so Go's assertion names no static interface type.
        (
            TypeId::GEO,
            Err("interface conversion: interface is nil, not geom.T"),
        ),
        (TypeId::UID, Err("invalid type for MarshalJSON: 7")),
        (
            TypeId::PASSWORD,
            Err("interface conversion: interface {} is nil, not string"),
        ),
        (TypeId::STRING, Ok(r#""""#)),
        (TypeId::OBJECT, Err("invalid type for MarshalJSON: 10")),
        (
            TypeId::BIGFLOAT,
            Err("interface conversion: interface {} is nil, not big.Float"),
        ),
        (TypeId::VFLOAT, Err("invalid type for MarshalJSON: 12")),
        (TypeId::new(13), Err("invalid type for MarshalJSON: 13")),
        (TypeId::UNDEFINED, Err("invalid type for MarshalJSON: 100")),
        (TypeId::new(255), Err("invalid type for MarshalJSON: 255")),
    ];
    let failures: Vec<String> = cases
        .into_iter()
        .filter_map(|(id, want)| {
            let got = marshal_json(&Val::nil(id))
                .map(|b| String::from_utf8(b).unwrap())
                .map_err(|e| e.to_string());
            let want = want.map(String::from).map_err(String::from);
            (got != want).then(|| format!("{id:?}: got {got:?}, Go {want:?}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// One value of each Go type the port's `Value` stands for, as Go's `Val.Value` holds it: `[]uint8`, `string`,
/// `int64`, `float64`, `bool`, `time.Time`, a pointer to each of go-geom's geometry types, `uint64`, `big.Float` and
/// `[]float32`.
fn marshal_json_values() -> Vec<(&'static str, Value)> {
    use ultragraphdb_core_types::{BigFloat, Geometry, Layout};
    let ring = || {
        vec![
            vec![0.0, 0.0],
            vec![1.0, 0.0],
            vec![0.0, 1.0],
            vec![0.0, 0.0],
        ]
    };
    let line = || vec![vec![1.0, 2.0], vec![3.0, 4.0]];
    let point = || Geometry::point(Layout::XY, &[1.0, 2.0]).unwrap();
    vec![
        ("Bytes", Value::Bytes(b"ab".to_vec())),
        ("Str", Value::str("a")),
        ("Int", Value::Int(1)),
        ("Float", Value::Float(1.5)),
        ("Bool", Value::Bool(true)),
        (
            "DateTime",
            Value::DateTime(DateTime::utc(2006, 1, 2, 15, 4, 5, 0)),
        ),
        ("Point", Value::Geo(point())),
        (
            "LineString",
            Value::Geo(Geometry::line_string(Layout::XY, &line()).unwrap()),
        ),
        (
            "Polygon",
            Value::Geo(Geometry::polygon(Layout::XY, &[ring()]).unwrap()),
        ),
        (
            "MultiPoint",
            Value::Geo(Geometry::multi_point(Layout::XY, &[Some(vec![1.0, 2.0])]).unwrap()),
        ),
        (
            "MultiLineString",
            Value::Geo(Geometry::multi_line_string(Layout::XY, &[line()]).unwrap()),
        ),
        (
            "MultiPolygon",
            Value::Geo(Geometry::multi_polygon(Layout::XY, &[vec![ring()]]).unwrap()),
        ),
        (
            "GeometryCollection",
            Value::Geo(Geometry::collection(vec![point()])),
        ),
        ("Uid", Value::Uid(7)),
        (
            "BigFloat",
            Value::BigFloat(BigFloat::unmarshal_text("1.5", 200).unwrap()),
        ),
        ("VFloat", Value::VFloat(vec![1.0, 2.0])),
    ]
}

/// Go v25.5.0's `Val.MarshalJSON` of every type identifier over a value of each Go type the port's `Value` stands
/// for, as Go 1.27 answers `Val{Tid: id, Value: v}.MarshalJSON()`, the values in [`marshal_json_values`]' order.
/// Deviation: where Go panics on its type assertion (`interface conversion: ...`), the port returns the panic's
/// text. Binary, uid, object, vfloat and unknown types are Go's refusal whatever the value; a string or default
/// asserts the value `Safe` returns, which is the value itself when it is not nil.
#[test]
fn test_marshal_json_mismatched_value() {
    use ultragraphdb_core_types::marshal_json;
    let values = marshal_json_values();
    let cases: [(TypeId, [Result<&str, &str>; 16]); 14] = [
        (
            TypeId::DEFAULT,
            [
                Err("interface conversion: interface {} is []uint8, not string"),
                Ok(r#""a""#),
                Err("interface conversion: interface {} is int64, not string"),
                Err("interface conversion: interface {} is float64, not string"),
                Err("interface conversion: interface {} is bool, not string"),
                Err("interface conversion: interface {} is time.Time, not string"),
                Err("interface conversion: interface {} is *geom.Point, not string"),
                Err("interface conversion: interface {} is *geom.LineString, not string"),
                Err("interface conversion: interface {} is *geom.Polygon, not string"),
                Err("interface conversion: interface {} is *geom.MultiPoint, not string"),
                Err("interface conversion: interface {} is *geom.MultiLineString, not string"),
                Err("interface conversion: interface {} is *geom.MultiPolygon, not string"),
                Err("interface conversion: interface {} is *geom.GeometryCollection, not string"),
                Err("interface conversion: interface {} is uint64, not string"),
                Err("interface conversion: interface {} is big.Float, not string"),
                Err("interface conversion: interface {} is []float32, not string"),
            ],
        ),
        (
            TypeId::BINARY,
            [
                Err("invalid type for MarshalJSON: 1"),
                Err("invalid type for MarshalJSON: 1"),
                Err("invalid type for MarshalJSON: 1"),
                Err("invalid type for MarshalJSON: 1"),
                Err("invalid type for MarshalJSON: 1"),
                Err("invalid type for MarshalJSON: 1"),
                Err("invalid type for MarshalJSON: 1"),
                Err("invalid type for MarshalJSON: 1"),
                Err("invalid type for MarshalJSON: 1"),
                Err("invalid type for MarshalJSON: 1"),
                Err("invalid type for MarshalJSON: 1"),
                Err("invalid type for MarshalJSON: 1"),
                Err("invalid type for MarshalJSON: 1"),
                Err("invalid type for MarshalJSON: 1"),
                Err("invalid type for MarshalJSON: 1"),
                Err("invalid type for MarshalJSON: 1"),
            ],
        ),
        (
            TypeId::INT,
            [
                Err("interface conversion: interface {} is []uint8, not int64"),
                Err("interface conversion: interface {} is string, not int64"),
                Ok("1"),
                Err("interface conversion: interface {} is float64, not int64"),
                Err("interface conversion: interface {} is bool, not int64"),
                Err("interface conversion: interface {} is time.Time, not int64"),
                Err("interface conversion: interface {} is *geom.Point, not int64"),
                Err("interface conversion: interface {} is *geom.LineString, not int64"),
                Err("interface conversion: interface {} is *geom.Polygon, not int64"),
                Err("interface conversion: interface {} is *geom.MultiPoint, not int64"),
                Err("interface conversion: interface {} is *geom.MultiLineString, not int64"),
                Err("interface conversion: interface {} is *geom.MultiPolygon, not int64"),
                Err("interface conversion: interface {} is *geom.GeometryCollection, not int64"),
                Err("interface conversion: interface {} is uint64, not int64"),
                Err("interface conversion: interface {} is big.Float, not int64"),
                Err("interface conversion: interface {} is []float32, not int64"),
            ],
        ),
        (
            TypeId::FLOAT,
            [
                Err("interface conversion: interface {} is []uint8, not float64"),
                Err("interface conversion: interface {} is string, not float64"),
                Err("interface conversion: interface {} is int64, not float64"),
                Ok("1.5"),
                Err("interface conversion: interface {} is bool, not float64"),
                Err("interface conversion: interface {} is time.Time, not float64"),
                Err("interface conversion: interface {} is *geom.Point, not float64"),
                Err("interface conversion: interface {} is *geom.LineString, not float64"),
                Err("interface conversion: interface {} is *geom.Polygon, not float64"),
                Err("interface conversion: interface {} is *geom.MultiPoint, not float64"),
                Err("interface conversion: interface {} is *geom.MultiLineString, not float64"),
                Err("interface conversion: interface {} is *geom.MultiPolygon, not float64"),
                Err("interface conversion: interface {} is *geom.GeometryCollection, not float64"),
                Err("interface conversion: interface {} is uint64, not float64"),
                Err("interface conversion: interface {} is big.Float, not float64"),
                Err("interface conversion: interface {} is []float32, not float64"),
            ],
        ),
        (
            TypeId::BOOL,
            [
                Err("interface conversion: interface {} is []uint8, not bool"),
                Err("interface conversion: interface {} is string, not bool"),
                Err("interface conversion: interface {} is int64, not bool"),
                Err("interface conversion: interface {} is float64, not bool"),
                Ok("true"),
                Err("interface conversion: interface {} is time.Time, not bool"),
                Err("interface conversion: interface {} is *geom.Point, not bool"),
                Err("interface conversion: interface {} is *geom.LineString, not bool"),
                Err("interface conversion: interface {} is *geom.Polygon, not bool"),
                Err("interface conversion: interface {} is *geom.MultiPoint, not bool"),
                Err("interface conversion: interface {} is *geom.MultiLineString, not bool"),
                Err("interface conversion: interface {} is *geom.MultiPolygon, not bool"),
                Err("interface conversion: interface {} is *geom.GeometryCollection, not bool"),
                Err("interface conversion: interface {} is uint64, not bool"),
                Err("interface conversion: interface {} is big.Float, not bool"),
                Err("interface conversion: interface {} is []float32, not bool"),
            ],
        ),
        (
            TypeId::DATETIME,
            [
                Err("interface conversion: interface {} is []uint8, not time.Time"),
                Err("interface conversion: interface {} is string, not time.Time"),
                Err("interface conversion: interface {} is int64, not time.Time"),
                Err("interface conversion: interface {} is float64, not time.Time"),
                Err("interface conversion: interface {} is bool, not time.Time"),
                Ok(r#""2006-01-02T15:04:05Z""#),
                Err("interface conversion: interface {} is *geom.Point, not time.Time"),
                Err("interface conversion: interface {} is *geom.LineString, not time.Time"),
                Err("interface conversion: interface {} is *geom.Polygon, not time.Time"),
                Err("interface conversion: interface {} is *geom.MultiPoint, not time.Time"),
                Err("interface conversion: interface {} is *geom.MultiLineString, not time.Time"),
                Err("interface conversion: interface {} is *geom.MultiPolygon, not time.Time"),
                Err(
                    "interface conversion: interface {} is *geom.GeometryCollection, not time.Time",
                ),
                Err("interface conversion: interface {} is uint64, not time.Time"),
                Err("interface conversion: interface {} is big.Float, not time.Time"),
                Err("interface conversion: interface {} is []float32, not time.Time"),
            ],
        ),
        (
            TypeId::GEO,
            [
                Err("interface conversion: []uint8 is not geom.T: missing method Bounds"),
                Err("interface conversion: string is not geom.T: missing method Bounds"),
                Err("interface conversion: int64 is not geom.T: missing method Bounds"),
                Err("interface conversion: float64 is not geom.T: missing method Bounds"),
                Err("interface conversion: bool is not geom.T: missing method Bounds"),
                Err("interface conversion: time.Time is not geom.T: missing method Bounds"),
                Ok(r#"{"type":"Point","coordinates":[1,2]}"#),
                Ok(r#"{"type":"LineString","coordinates":[[1,2],[3,4]]}"#),
                Ok(r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[0,1],[0,0]]]}"#),
                Ok(r#"{"type":"MultiPoint","coordinates":[[1,2]]}"#),
                Ok(r#"{"type":"MultiLineString","coordinates":[[[1,2],[3,4]]]}"#),
                Ok(r#"{"type":"MultiPolygon","coordinates":[[[[0,0],[1,0],[0,1],[0,0]]]]}"#),
                Ok(
                    r#"{"type":"GeometryCollection","geometries":[{"type":"Point","coordinates":[1,2]}]}"#,
                ),
                Err("interface conversion: uint64 is not geom.T: missing method Bounds"),
                Err("interface conversion: big.Float is not geom.T: missing method Bounds"),
                Err("interface conversion: []float32 is not geom.T: missing method Bounds"),
            ],
        ),
        (
            TypeId::UID,
            [
                Err("invalid type for MarshalJSON: 7"),
                Err("invalid type for MarshalJSON: 7"),
                Err("invalid type for MarshalJSON: 7"),
                Err("invalid type for MarshalJSON: 7"),
                Err("invalid type for MarshalJSON: 7"),
                Err("invalid type for MarshalJSON: 7"),
                Err("invalid type for MarshalJSON: 7"),
                Err("invalid type for MarshalJSON: 7"),
                Err("invalid type for MarshalJSON: 7"),
                Err("invalid type for MarshalJSON: 7"),
                Err("invalid type for MarshalJSON: 7"),
                Err("invalid type for MarshalJSON: 7"),
                Err("invalid type for MarshalJSON: 7"),
                Err("invalid type for MarshalJSON: 7"),
                Err("invalid type for MarshalJSON: 7"),
                Err("invalid type for MarshalJSON: 7"),
            ],
        ),
        (
            TypeId::PASSWORD,
            [
                Err("interface conversion: interface {} is []uint8, not string"),
                Ok(r#""a""#),
                Err("interface conversion: interface {} is int64, not string"),
                Err("interface conversion: interface {} is float64, not string"),
                Err("interface conversion: interface {} is bool, not string"),
                Err("interface conversion: interface {} is time.Time, not string"),
                Err("interface conversion: interface {} is *geom.Point, not string"),
                Err("interface conversion: interface {} is *geom.LineString, not string"),
                Err("interface conversion: interface {} is *geom.Polygon, not string"),
                Err("interface conversion: interface {} is *geom.MultiPoint, not string"),
                Err("interface conversion: interface {} is *geom.MultiLineString, not string"),
                Err("interface conversion: interface {} is *geom.MultiPolygon, not string"),
                Err("interface conversion: interface {} is *geom.GeometryCollection, not string"),
                Err("interface conversion: interface {} is uint64, not string"),
                Err("interface conversion: interface {} is big.Float, not string"),
                Err("interface conversion: interface {} is []float32, not string"),
            ],
        ),
        (
            TypeId::STRING,
            [
                Err("interface conversion: interface {} is []uint8, not string"),
                Ok(r#""a""#),
                Err("interface conversion: interface {} is int64, not string"),
                Err("interface conversion: interface {} is float64, not string"),
                Err("interface conversion: interface {} is bool, not string"),
                Err("interface conversion: interface {} is time.Time, not string"),
                Err("interface conversion: interface {} is *geom.Point, not string"),
                Err("interface conversion: interface {} is *geom.LineString, not string"),
                Err("interface conversion: interface {} is *geom.Polygon, not string"),
                Err("interface conversion: interface {} is *geom.MultiPoint, not string"),
                Err("interface conversion: interface {} is *geom.MultiLineString, not string"),
                Err("interface conversion: interface {} is *geom.MultiPolygon, not string"),
                Err("interface conversion: interface {} is *geom.GeometryCollection, not string"),
                Err("interface conversion: interface {} is uint64, not string"),
                Err("interface conversion: interface {} is big.Float, not string"),
                Err("interface conversion: interface {} is []float32, not string"),
            ],
        ),
        (
            TypeId::OBJECT,
            [
                Err("invalid type for MarshalJSON: 10"),
                Err("invalid type for MarshalJSON: 10"),
                Err("invalid type for MarshalJSON: 10"),
                Err("invalid type for MarshalJSON: 10"),
                Err("invalid type for MarshalJSON: 10"),
                Err("invalid type for MarshalJSON: 10"),
                Err("invalid type for MarshalJSON: 10"),
                Err("invalid type for MarshalJSON: 10"),
                Err("invalid type for MarshalJSON: 10"),
                Err("invalid type for MarshalJSON: 10"),
                Err("invalid type for MarshalJSON: 10"),
                Err("invalid type for MarshalJSON: 10"),
                Err("invalid type for MarshalJSON: 10"),
                Err("invalid type for MarshalJSON: 10"),
                Err("invalid type for MarshalJSON: 10"),
                Err("invalid type for MarshalJSON: 10"),
            ],
        ),
        (
            TypeId::BIGFLOAT,
            [
                Err("interface conversion: interface {} is []uint8, not big.Float"),
                Err("interface conversion: interface {} is string, not big.Float"),
                Err("interface conversion: interface {} is int64, not big.Float"),
                Err("interface conversion: interface {} is float64, not big.Float"),
                Err("interface conversion: interface {} is bool, not big.Float"),
                Err("interface conversion: interface {} is time.Time, not big.Float"),
                Err("interface conversion: interface {} is *geom.Point, not big.Float"),
                Err("interface conversion: interface {} is *geom.LineString, not big.Float"),
                Err("interface conversion: interface {} is *geom.Polygon, not big.Float"),
                Err("interface conversion: interface {} is *geom.MultiPoint, not big.Float"),
                Err("interface conversion: interface {} is *geom.MultiLineString, not big.Float"),
                Err("interface conversion: interface {} is *geom.MultiPolygon, not big.Float"),
                Err(
                    "interface conversion: interface {} is *geom.GeometryCollection, not big.Float",
                ),
                Err("interface conversion: interface {} is uint64, not big.Float"),
                Ok("1.5"),
                Err("interface conversion: interface {} is []float32, not big.Float"),
            ],
        ),
        (
            TypeId::VFLOAT,
            [
                Err("invalid type for MarshalJSON: 12"),
                Err("invalid type for MarshalJSON: 12"),
                Err("invalid type for MarshalJSON: 12"),
                Err("invalid type for MarshalJSON: 12"),
                Err("invalid type for MarshalJSON: 12"),
                Err("invalid type for MarshalJSON: 12"),
                Err("invalid type for MarshalJSON: 12"),
                Err("invalid type for MarshalJSON: 12"),
                Err("invalid type for MarshalJSON: 12"),
                Err("invalid type for MarshalJSON: 12"),
                Err("invalid type for MarshalJSON: 12"),
                Err("invalid type for MarshalJSON: 12"),
                Err("invalid type for MarshalJSON: 12"),
                Err("invalid type for MarshalJSON: 12"),
                Err("invalid type for MarshalJSON: 12"),
                Err("invalid type for MarshalJSON: 12"),
            ],
        ),
        (
            TypeId::new(13),
            [
                Err("invalid type for MarshalJSON: 13"),
                Err("invalid type for MarshalJSON: 13"),
                Err("invalid type for MarshalJSON: 13"),
                Err("invalid type for MarshalJSON: 13"),
                Err("invalid type for MarshalJSON: 13"),
                Err("invalid type for MarshalJSON: 13"),
                Err("invalid type for MarshalJSON: 13"),
                Err("invalid type for MarshalJSON: 13"),
                Err("invalid type for MarshalJSON: 13"),
                Err("invalid type for MarshalJSON: 13"),
                Err("invalid type for MarshalJSON: 13"),
                Err("invalid type for MarshalJSON: 13"),
                Err("invalid type for MarshalJSON: 13"),
                Err("invalid type for MarshalJSON: 13"),
                Err("invalid type for MarshalJSON: 13"),
                Err("invalid type for MarshalJSON: 13"),
            ],
        ),
    ];
    let failures: Vec<String> = cases
        .into_iter()
        .flat_map(|(id, wants)| {
            values
                .iter()
                .zip(wants)
                .filter_map(move |((kind, v), want)| {
                    let got = marshal_json(&Val::new(id, v.clone()))
                        .map(|b| String::from_utf8(b).unwrap())
                        .map_err(|e| e.to_string());
                    let want = want.map(String::from).map_err(String::from);
                    (got != want).then(|| format!("{id:?} of {kind}: got {got:?}, Go {want:?}"))
                })
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Go v25.5.0's `Val.MarshalJSON` of a geometry with a non-finite coordinate, as Go 1.27 answers it through
/// go-geom's `geojson.Marshal`: the error names the first non-finite coordinate in output order, members of
/// collections in order and nested collections depth first.
#[test]
fn test_marshal_json_geo_non_finite() {
    use ultragraphdb_core_types::{Geometry, Layout, marshal_json};
    const INF: f64 = f64::INFINITY;
    const NINF: f64 = f64::NEG_INFINITY;
    const NAN: f64 = f64::NAN;
    let pt = |x: f64, y: f64| Geometry::point(Layout::XY, &[x, y]).unwrap();
    let line = |c: &[[f64; 2]]| c.iter().map(|c| c.to_vec()).collect::<Vec<_>>();
    let cases: Vec<(&str, Geometry, &str)> = vec![
        ("point +Inf first", pt(INF, 1.0), "+Inf"),
        ("point -Inf second", pt(1.0, NINF), "-Inf"),
        ("point NaN second", pt(1.0, NAN), "NaN"),
        ("point NaN then +Inf", pt(NAN, INF), "NaN"),
        ("point -Inf then NaN", pt(NINF, NAN), "-Inf"),
        (
            "point xyz +Inf third",
            Geometry::point(Layout::XYZ, &[1.0, 2.0, INF]).unwrap(),
            "+Inf",
        ),
        (
            "linestring +Inf in second coord",
            Geometry::line_string(Layout::XY, &line(&[[1.0, 2.0], [3.0, INF]])).unwrap(),
            "+Inf",
        ),
        (
            "linestring -Inf first then NaN",
            Geometry::line_string(Layout::XY, &line(&[[NINF, 2.0], [NAN, 3.0]])).unwrap(),
            "-Inf",
        ),
        (
            "linestring NaN first",
            Geometry::line_string(Layout::XY, &line(&[[NAN, 2.0], [INF, 3.0]])).unwrap(),
            "NaN",
        ),
        (
            "polygon NaN then +Inf",
            Geometry::polygon(
                Layout::XY,
                &[line(&[[0.0, 0.0], [1.0, NAN], [INF, 0.0], [0.0, 0.0]])],
            )
            .unwrap(),
            "NaN",
        ),
        (
            "polygon second ring -Inf",
            Geometry::polygon(
                Layout::XY,
                &[
                    line(&[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [0.0, 0.0]]),
                    line(&[[0.0, 0.0], [NINF, 0.0], [0.0, NAN], [0.0, 0.0]]),
                ],
            )
            .unwrap(),
            "-Inf",
        ),
        (
            "polygon +Inf first",
            Geometry::polygon(Layout::XY, &[line(&[[INF, 0.0], [NINF, 0.0], [0.0, 0.0]])]).unwrap(),
            "+Inf",
        ),
        (
            "multipoint NaN after empty",
            Geometry::multi_point(
                Layout::XY,
                &[
                    Some(vec![1.0, 2.0]),
                    None,
                    Some(vec![NAN, 1.0]),
                    Some(vec![INF, 1.0]),
                ],
            )
            .unwrap(),
            "NaN",
        ),
        (
            "multipoint -Inf first",
            Geometry::multi_point(Layout::XY, &[Some(vec![NINF, 2.0]), Some(vec![NAN, 1.0])])
                .unwrap(),
            "-Inf",
        ),
        (
            "multilinestring -Inf in second line",
            Geometry::multi_line_string(
                Layout::XY,
                &[line(&[[1.0, 2.0]]), line(&[[3.0, NINF], [NAN, 0.0]])],
            )
            .unwrap(),
            "-Inf",
        ),
        (
            "multilinestring +Inf first",
            Geometry::multi_line_string(Layout::XY, &[line(&[[INF, 2.0]]), line(&[[3.0, NINF]])])
                .unwrap(),
            "+Inf",
        ),
        (
            "multipolygon +Inf in second polygon",
            Geometry::multi_polygon(
                Layout::XY,
                &[
                    vec![line(&[[0.0, 0.0], [1.0, 0.0], [0.0, 0.0]])],
                    vec![line(&[[0.0, 0.0], [INF, NAN], [0.0, 0.0]])],
                ],
            )
            .unwrap(),
            "+Inf",
        ),
        (
            "multipolygon NaN first",
            Geometry::multi_polygon(
                Layout::XY,
                &[
                    vec![line(&[[NAN, 0.0], [1.0, 0.0], [0.0, 0.0]])],
                    vec![line(&[[0.0, 0.0], [INF, 0.0], [0.0, 0.0]])],
                ],
            )
            .unwrap(),
            "NaN",
        ),
        (
            "multipolygon -Inf after empty polygon",
            Geometry::multi_polygon(
                Layout::XY,
                &[vec![], vec![line(&[[0.0, 0.0], [0.0, NINF], [0.0, 0.0]])]],
            )
            .unwrap(),
            "-Inf",
        ),
        (
            "collection -Inf in second member",
            Geometry::collection(vec![
                pt(1.0, 2.0),
                Geometry::line_string(Layout::XY, &line(&[[1.0, 2.0], [NINF, 0.0]])).unwrap(),
                pt(NAN, 0.0),
            ]),
            "-Inf",
        ),
        (
            "collection nested NaN before +Inf",
            Geometry::collection(vec![
                Geometry::collection(vec![pt(1.0, 1.0)]),
                Geometry::collection(vec![pt(NAN, 0.0)]),
                pt(INF, 0.0),
            ]),
            "NaN",
        ),
        (
            "collection +Inf first member",
            Geometry::collection(vec![pt(INF, 0.0), pt(NAN, 0.0)]),
            "+Inf",
        ),
        (
            "point xym NaN m",
            Geometry::point(Layout::XYM, &[1.0, 2.0, NAN]).unwrap(),
            "NaN",
        ),
        (
            "point xyzm -Inf m",
            Geometry::point(Layout::XYZM, &[1.0, 2.0, 3.0, NINF]).unwrap(),
            "-Inf",
        ),
        (
            "linestring xyzm NaN z before +Inf m",
            Geometry::line_string(
                Layout::XYZM,
                &[vec![1.0, 2.0, 3.0, 4.0], vec![5.0, 6.0, NAN, INF]],
            )
            .unwrap(),
            "NaN",
        ),
        (
            "multipoint xyz NaN z in second point",
            Geometry::multi_point(
                Layout::XYZ,
                &[Some(vec![1.0, 2.0, 3.0]), Some(vec![4.0, 5.0, NAN])],
            )
            .unwrap(),
            "NaN",
        ),
        (
            "multilinestring +Inf in third line",
            Geometry::multi_line_string(
                Layout::XY,
                &[
                    line(&[[1.0, 2.0], [3.0, 4.0]]),
                    line(&[[5.0, 6.0]]),
                    line(&[[7.0, 8.0], [9.0, INF]]),
                ],
            )
            .unwrap(),
            "+Inf",
        ),
        (
            "collection thrice nested -Inf in second ring before +Inf",
            Geometry::collection(vec![
                Geometry::collection(vec![Geometry::collection(vec![
                    Geometry::multi_polygon(
                        Layout::XY,
                        &[vec![
                            line(&[[0.0, 0.0], [1.0, 0.0], [0.0, 0.0]]),
                            line(&[[0.0, 0.0], [0.0, NINF], [NAN, 0.0]]),
                        ]],
                    )
                    .unwrap(),
                ])]),
                pt(INF, 0.0),
            ]),
            "-Inf",
        ),
        (
            "collection +Inf after empty members",
            Geometry::collection(vec![
                Geometry::collection(vec![]),
                Geometry::point_empty(Layout::XY),
                Geometry::multi_point(Layout::XY, &[]).unwrap(),
                pt(1.0, INF),
            ]),
            "+Inf",
        ),
    ];
    let failures: Vec<String> = cases
        .into_iter()
        .filter_map(|(name, g, what)| {
            let got =
                marshal_json(&Val::new(TypeId::GEO, Value::Geo(g))).map_err(|e| e.to_string());
            let want = Err(format!("json: unsupported value: {what}"));
            (got != want).then(|| format!("{name}: got {got:?}, Go {want:?}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Go v25.5.0's `ParseVFloat` on inputs at its edges: any run of closing brackets, blank lists, separators that
/// mix or repeat, and what Go's `ParseFloat(_, 32)` accepts (hex, underscores, the infinities, NaN, underflow).
#[test]
fn test_parse_vfloat() {
    use ultragraphdb_core_types::parse_vfloat;
    let cases: [(&str, Option<&[f32]>); 20] = [
        ("[1,2]]]", Some(&[1.0, 2.0])),
        ("[", None),
        ("]", None),
        ("[ ]", Some(&[])),
        ("[1,,2]", None),
        (" [1 , 2] ", Some(&[1.0, 2.0])),
        ("[1\t2\n3]", Some(&[1.0, 2.0, 3.0])),
        ("[[1]", None),
        ("[1 2,3]", None),
        ("[-0, 3.4e38]", Some(&[-0.0, 3.4e38])),
        ("[1e39]", None),
        ("[0x1p-2]", Some(&[0.25])),
        ("[1_0]", Some(&[10.0])),
        (
            "[inf, -Inf, NaN]",
            Some(&[f32::INFINITY, f32::NEG_INFINITY, f32::NAN]),
        ),
        ("[1]x", None),
        ("x[1]", None),
        ("[1e-46]", Some(&[0.0])),
        ("[,]", None),
        ("[1,]", None),
        ("[  1   2  ]", Some(&[1.0, 2.0])),
    ];
    let bits = |v: &[f32]| v.iter().map(|f| f.to_bits()).collect::<Vec<_>>();
    for (input, want) in cases {
        match (parse_vfloat(input), want) {
            (Ok(got), Some(want)) => assert_eq!(bits(&got), bits(want), "{input:?}"),
            (Err(e), None) => assert_eq!(
                e.to_string(),
                format!("cannot convert {} to vfloat", input.trim()),
                "{input:?}"
            ),
            (got, want) => panic!("{input:?}: got {got:?}, Go {want:?}"),
        }
    }
}

/// Go v25.5.0's `Val.MarshalJSON`, as Go 1.27 writes it: `encoding/json`'s float layout and escapes, a time's
/// RFC 3339 form and its range error, a bigfloat's `MarshalText`, and the type identifier of every type it refuses.
#[test]
fn test_marshal_json_go_texts() {
    use ultragraphdb_core_types::{BigFloat, marshal_json};
    let json = |v: Val| {
        marshal_json(&v)
            .map(|b| String::from_utf8(b).unwrap())
            .map_err(|e| e.to_string())
    };
    let float = |f: f64| Val::new(TypeId::FLOAT, Value::Float(f));
    let big = |s: &str| {
        Val::new(
            TypeId::BIGFLOAT,
            Value::BigFloat(BigFloat::unmarshal_text(s, 200).unwrap()),
        )
    };
    let year_range =
        "json: error calling MarshalJSON for type *time.Time: year outside of range [0,9999]";
    let cases: Vec<(Val, Result<&str, &str>)> = vec![
        (
            Val::new(TypeId::INT, Value::Int(i64::MIN)),
            Ok("-9223372036854775808"),
        ),
        (float(-0.0), Ok("-0")),
        (float(1e-7), Ok("1e-7")),
        (float(1e-6), Ok("0.000001")),
        (float(1e20), Ok("100000000000000000000")),
        (float(1e21), Ok("1e+21")),
        (float(123456789.0), Ok("123456789")),
        (float(f64::MAX), Ok("1.7976931348623157e+308")),
        (float(5e-324), Ok("5e-324")),
        (float(f64::INFINITY), Err("json: unsupported value: +Inf")),
        (
            float(f64::NEG_INFINITY),
            Err("json: unsupported value: -Inf"),
        ),
        (
            Val::new(
                TypeId::DATETIME,
                Value::DateTime(DateTime::utc(10000, 1, 1, 0, 0, 0, 0)),
            ),
            Err(year_range),
        ),
        (
            Val::new(
                TypeId::DATETIME,
                Value::DateTime(DateTime::utc(-1, 1, 1, 0, 0, 0, 0)),
            ),
            Err(year_range),
        ),
        (
            Val::new(
                TypeId::DATETIME,
                Value::DateTime(DateTime::new(
                    2018,
                    5,
                    30,
                    9,
                    30,
                    10,
                    500_000_000,
                    Zone::Fixed(-6 * 3600),
                )),
            ),
            Ok(r#""2018-05-30T09:30:10.5-06:00""#),
        ),
        (
            Val::new(TypeId::PASSWORD, s("p\"w<")),
            // Go escapes <, > and & as \u003c, \u003e and \u0026.
            Ok(concat!("\"p\\\"w\\", "u003c\"")),
        ),
        (Val::new(TypeId::STRING, s("")), Ok(r#""""#)),
        (
            Val::new(TypeId::DEFAULT, s("日本\u{2028}")),
            Ok(concat!("\"日本\\", "u2028\"")),
        ),
        (
            Val::new(
                TypeId::DATETIME,
                Value::DateTime(DateTime::new(
                    2018,
                    1,
                    1,
                    0,
                    0,
                    0,
                    0,
                    Zone::Fixed(25 * 3600),
                )),
            ),
            Err(
                "json: error calling MarshalJSON for type *time.Time: timezone hour outside of range [0,23]",
            ),
        ),
        (
            Val::new(
                TypeId::DATETIME,
                Value::DateTime(DateTime::new(2018, 1, 1, 0, 0, 0, 0, Zone::Fixed(30))),
            ),
            Ok(r#""2018-01-01T00:00:00+00:00""#),
        ),
        (Val::new(TypeId::BOOL, Value::Bool(false)), Ok("false")),
        (big("1e100"), Ok("1e+100")),
        (big("-0.125"), Ok("-0.125")),
        (
            Val::nil(TypeId::UID),
            Err("invalid type for MarshalJSON: 7"),
        ),
        (
            Val::nil(TypeId::VFLOAT),
            Err("invalid type for MarshalJSON: 12"),
        ),
        (
            Val::nil(TypeId::BINARY),
            Err("invalid type for MarshalJSON: 1"),
        ),
        (
            Val::nil(TypeId::OBJECT),
            Err("invalid type for MarshalJSON: 10"),
        ),
        (
            Val::new(TypeId::BINARY, Value::Bytes(b"ab".to_vec())),
            Err("invalid type for MarshalJSON: 1"),
        ),
    ];
    let failures: Vec<String> = cases
        .into_iter()
        .filter_map(|(v, want)| {
            let got = json(v.clone());
            let want = want.map(String::from).map_err(String::from);
            (got != want).then(|| format!("{v:?}: got {got:?}, Go {want:?}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
