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
    DateTime, TypeId, Val, Value, Zone, convert, float_array_as_bytes, marshal, object_value,
    verify_password,
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
fn test_object_value_password_error_does_not_leak_value() {
    let err = object_value(TypeId::PASSWORD, &Value::Int(123456789))
        .unwrap_err()
        .to_string();
    assert!(err.contains("Expected value of type password"));
    assert!(err.contains("int64"));
    assert!(!err.contains("123456789"));
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
        Some(Value::Str(s)) => Ok(s),
        None => Err("<nil>".into()),
        Some(other) => panic!("marshal gave {other:?}"),
    }
}

fn check_rendered(
    c: &ConvCase,
    input_plain: &[u8],
    out: &Val,
    want: &Rendered,
) -> Result<(), String> {
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
            Ok(())
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
        if !h.starts_with(b"$2a$") {
            return Err(format!("{ctx}: hash {h:?}"));
        }
        if c.from == TypeId::PASSWORD.enum_value() {
            // A password value converted to itself is the stored hash, unchanged.
            return if h[..7] == unhex(&want.str) {
                Ok(())
            } else {
                Err(format!("{ctx}: {h:?}"))
            };
        }
        return verify_password(input_plain, h).map_err(|e| format!("{ctx}: {e}"));
    }
    if !want.panic.is_empty() {
        // Go panics asserting a *big.Float; this port marshals the value, checked above.
        return Ok(());
    }
    let bin = marshal_bytes(out, TypeId::BINARY);
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
    let st = marshal_bytes(out, TypeId::STRING);
    match &st {
        Ok(b) if want.str_err.is_empty() && *b == unhex(&want.str) => {}
        Err(e) if !want.str_err.is_empty() && *e == want.str_err => {}
        _ => {
            return Err(format!(
                "{ctx}: string {:?}, Go {:?} / {}",
                st.as_ref().map(|b| String::from_utf8_lossy(b).into_owned()),
                String::from_utf8_lossy(&unhex(&want.str)),
                want.str_err
            ));
        }
    }
    Ok(())
}

#[test]
fn test_golden_conversion_matrix() {
    let path = ultragraphdb_test_bdd::testdata_dir("ultragraphdb_core_types")
        .join("golden/types_convert.json");
    let golden = ultragraphdb_test_bdd::load_golden::<ConvCase>(&path);
    let mut failures = Vec::new();
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
        let out = convert(&input, TypeId::new(c.to));
        let res = match (&out, &c.out) {
            (Err(e), None) if !c.err.is_empty() && e.to_string() == c.err => Ok(()),
            (Err(e), None) if !c.panic.is_empty() && e.to_string() == c.panic => Ok(()),
            (Ok(v), Some(want)) => check_rendered(c, &plain, v, want),
            _ => Err(format!(
                "{} -> {} of {}: {:?}, Go err {:?} panic {:?}",
                c.from, c.to, c.input, out, c.err, c.panic
            )),
        };
        if let Err(e) = res {
            failures.push(e);
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        golden.cases().len(),
        failures.join("\n")
    );
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
    assert_eq!(
        j(Val::new(TypeId::STRING, Value::Str(vec![b'a', 0xff]))),
        r#""a\ufffd""#
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
