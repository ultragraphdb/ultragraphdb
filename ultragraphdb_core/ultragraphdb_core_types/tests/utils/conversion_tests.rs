/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/conversion_test.go. Changed for ultragraphdb: translated to Rust; plus the
 * golden conversion matrix (tools/golden/types.go, types_convert).
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_types::{
    CivilTime, DateTime, Geometry, Layout, TypeId, Val, Value, Zone, convert, float_array_as_bytes,
    marshal, verify_password,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

fn bs_bool(b: bool) -> Vec<u8> {
    vec![u8::from(b)]
}

fn bs_i64(v: i64) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn bs_f64(v: f64) -> Vec<u8> {
    v.to_bits().to_le_bytes().to_vec()
}

fn bs_time(t: DateTime) -> TestResult<Vec<u8>> {
    Ok(t.marshal_binary()?)
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
fn test_same_conversion_date_time() -> TestResult {
    for t in [date(2006, 1, 2, 15, 4, 5, 0), DateTime::zero()] {
        assert_eq!(
            conv(TypeId::BINARY, bs_time(t)?, TypeId::DATETIME),
            ok(TypeId::DATETIME, Value::DateTime(t))
        );
    }
    Ok(())
}

/// From type, from bytes (`None`: Go's nil value), to type, and the failure's substring or the expected value.
type EdgeCase = (TypeId, Option<Vec<u8>>, TypeId, Result<Value, &'static str>);

#[test]
fn test_conversion_edge_cases() -> TestResult {
    // (from type, from bytes or None, to type, failure substring or expected value)
    let cases = [
        binary_edge_cases(),
        scalar_edge_cases(),
        datetime_edge_cases()?,
        datetime_number_edge_cases()?,
    ]
    .concat();
    for (from, data, to, want) in cases {
        let input = match data {
            Some(d) => Val::bytes(from, d),
            None => Val::nil(from),
        };
        let out = convert(&input, to);
        match want {
            Err(failure) => {
                let err = out.err().context(failure)?.to_string();
                assert!(
                    err.contains(failure),
                    "{} to {}: {err} lacks {failure}",
                    from.name(),
                    to.name()
                );
            }
            Ok(v) => assert_eq!(out?, Val::new(to, v)),
        }
    }
    Ok(())
}

/// Conversions from binary.
fn binary_edge_cases() -> Vec<EdgeCase> {
    vec![
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
    ]
}

/// Conversions from string, int, float and bool.
fn scalar_edge_cases() -> Vec<EdgeCase> {
    vec![
        (
            TypeId::STRING,
            Some(vec![]),
            TypeId::INT,
            Err("cannot parse integer from empty string"),
        ),
        (
            TypeId::STRING,
            Some(vec![]),
            TypeId::FLOAT,
            Err("cannot parse float from empty string"),
        ),
        (
            TypeId::STRING,
            Some(vec![]),
            TypeId::BOOL,
            Err("provided string was not `true` or `false`"),
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
    ]
}

/// Conversions of datetime bytes to datetime, binary and string.
fn datetime_edge_cases() -> TestResult<Vec<EdgeCase>> {
    let zero = DateTime::zero();
    Ok(vec![
        (
            TypeId::DATETIME,
            Some(vec![]),
            TypeId::DATETIME,
            Err("Time.UnmarshalBinary: no data"),
        ),
        (
            TypeId::DATETIME,
            Some(bs_time(zero)?),
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
            Some(bs_time(zero)?),
            TypeId::BINARY,
            Ok(Value::Bytes(bs_time(zero)?)),
        ),
        (
            TypeId::DATETIME,
            Some(vec![]),
            TypeId::STRING,
            Err("Time.UnmarshalBinary: no data"),
        ),
        (
            TypeId::DATETIME,
            Some(bs_time(zero)?),
            TypeId::STRING,
            Ok(s("0001-01-01T00:00:00Z")),
        ),
    ])
}

/// Conversions of datetime bytes to default, int and float.
fn datetime_number_edge_cases() -> TestResult<Vec<EdgeCase>> {
    let zero = DateTime::zero();
    Ok(vec![
        (
            TypeId::DATETIME,
            Some(vec![]),
            TypeId::DEFAULT,
            Err("Time.UnmarshalBinary: no data"),
        ),
        (
            TypeId::DATETIME,
            Some(bs_time(zero)?),
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
            Some(bs_time(zero)?),
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
            Some(bs_time(zero)?),
            TypeId::FLOAT,
            Ok(Value::Float(zero.unix_nano() as f64 / 1e9)),
        ),
    ])
}

#[test]
fn test_convert_to_default() -> TestResult {
    let cases = [
        (TypeId::STRING, b"a".to_vec(), "a"),
        (TypeId::STRING, b"".to_vec(), ""),
        (TypeId::DEFAULT, b"abc".to_vec(), "abc"),
        (TypeId::BINARY, b"2016".to_vec(), "2016"),
        (TypeId::INT, bs_i64(3), "3"),
        (TypeId::FLOAT, bs_f64(-3.5), "-3.5"),
        (
            TypeId::DATETIME,
            bs_time(date(2006, 1, 2, 15, 4, 5, 0))?,
            "2006-01-02T15:04:05Z",
        ),
        (
            TypeId::DATETIME,
            bs_time(DateTime::zero())?,
            "0001-01-01T00:00:00Z",
        ),
    ];
    for (tid, data, want) in cases {
        assert_eq!(
            conv(tid, data, TypeId::DEFAULT),
            ok(TypeId::DEFAULT, s(want))
        );
    }
    Ok(())
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
fn test_convert_to_binary() -> TestResult {
    let t = bs_time(date(2006, 1, 2, 15, 4, 5, 0))?;
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
    Ok(())
}

#[test]
fn test_convert_from_binary() -> TestResult {
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
            bs_time(date(2016, 1, 1, 0, 0, 0, 0))?,
            TypeId::DATETIME,
            Value::DateTime(date(2016, 1, 1, 0, 0, 0, 0)),
        ),
        (
            bs_time(DateTime::zero())?,
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
    Ok(())
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
fn test_convert_date_time_to_string() -> TestResult {
    let cases = [
        (date(2006, 1, 2, 15, 4, 5, 0), "2006-01-02T15:04:05Z"),
        (date(2006, 1, 2, 0, 0, 0, 0), "2006-01-02T00:00:00Z"),
        (date(2006, 1, 1, 0, 0, 0, 0), "2006-01-01T00:00:00Z"),
    ];
    for (t, want) in cases {
        assert_eq!(
            conv(TypeId::DATETIME, bs_time(t)?, TypeId::STRING),
            ok(TypeId::STRING, s(want))
        );
    }
    Ok(())
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
fn test_convert_to_password() -> TestResult {
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
                let out = out.context("out")?;
                assert_eq!(out.tid(), TypeId::PASSWORD);
                let hash = match out.value() {
                    Some(Value::Str(hash)) => Ok(hash),
                    other => Err(format!("{other:?}")),
                }
                .context("no hash")?;
                verify_password(&plain, hash)?;
            }
        }
    }
    Ok(())
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
        ("True", Err("refused")),
        ("T", Err("refused")),
        ("F", Err("refused")),
        ("0", Ok(false)),
        ("false", Ok(false)),
        ("False", Err("refused")),
        ("", Err("refused")),
        ("srfrog", Err("refused")),
        ("3", Err("refused")),
        ("-3", Err("refused")),
    ];
    for (input, want) in cases {
        let out = conv(TypeId::STRING, input.into(), TypeId::BOOL);
        match want {
            Ok(b) => assert_eq!(out, ok(TypeId::BOOL, Value::Bool(b))),
            Err(_) => assert!(out.is_err(), "{input:?}: {out:?}"),
        }
    }
}

#[test]
fn test_convert_date_time_to_bool() -> TestResult {
    let now = DateTime::from_unix(1_700_000_000, 5);
    assert_eq!(
        conv(TypeId::DATETIME, bs_time(now)?, TypeId::BOOL),
        Err("Cannot convert datetime to type bool".into())
    );
    Ok(())
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
fn test_truthy() -> TestResult {
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
            conv(tid, data, TypeId::BOOL)
                .context("conv(tid, data, TypeId::BOOL)")?
                .value(),
            Some(&Value::Bool(true))
        );
    }
    Ok(())
}

#[test]
fn test_falsy() -> TestResult {
    let cases = [
        (TypeId::STRING, b"false".to_vec()),
        (TypeId::DEFAULT, b"false".to_vec()),
        (TypeId::INT, bs_i64(0)),
        (TypeId::FLOAT, bs_f64(0.0)),
        (TypeId::BINARY, Vec::new()),
    ];
    for (tid, data) in cases {
        assert_eq!(
            conv(tid, data, TypeId::BOOL)
                .context("conv(tid, data, TypeId::BOOL)")?
                .value(),
            Some(&Value::Bool(false))
        );
    }
    Ok(())
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
        ("", Err("refused")),
        ("srfrog", Err("refused")),
        ("3.0", Err("refused")),
        ("-3a.5", Err("refused")),
    ];
    for (input, want) in cases {
        let out = conv(TypeId::STRING, input.into(), TypeId::INT);
        match want {
            Ok(i) => assert_eq!(out, ok(TypeId::INT, Value::Int(i))),
            Err(_) => assert!(out.is_err(), "{input:?}: {out:?}"),
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
        ("", Err("refused")),
        ("srfrog", Err("refused")),
        ("-3a.5", Err("refused")),
        ("1e400", Err("refused")),
    ];
    for (input, want) in cases {
        let out = conv(TypeId::STRING, input.into(), TypeId::FLOAT);
        match want {
            Ok(f) => assert_eq!(out, ok(TypeId::FLOAT, Value::Float(f))),
            Err(_) => assert!(out.is_err(), "{input:?}: {out:?}"),
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

/// A decimal of seconds outside int64 is no datetime; one inside it is, its fraction kept.
#[test]
fn test_convert_decimal_to_date_time_outside_int64() {
    for text in ["9223372036854775808", "-9223372036854775809"] {
        assert_eq!(
            conv(TypeId::DECIMAL, text.into(), TypeId::DATETIME),
            Err("Float out of int64 range".into()),
            "{text}"
        );
    }
    for text in ["9223372036854775807.5", "-9223372036854775808", "1.5"] {
        assert!(
            conv(TypeId::DECIMAL, text.into(), TypeId::DATETIME).is_ok(),
            "{text}"
        );
    }
}

/// A decimal converts exactly: to and from its text, from an int, truncated toward zero to an int, and to a datetime's
/// nanoseconds; a text that is no `xsd:decimal`, an exponent included, is refused.
#[test]
fn test_convert_decimal() {
    let ok = |s: &str| Ok::<String, String>(s.into());
    assert_eq!(
        conv_text(TypeId::STRING, "1.50", TypeId::DECIMAL),
        ok("1.5")
    );
    assert_eq!(
        conv_text(TypeId::STRING, " -0.125 ", TypeId::DECIMAL),
        ok("-0.125")
    );
    assert_eq!(conv_text(TypeId::DECIMAL, "2.9", TypeId::INT), ok("2"));
    assert_eq!(conv_text(TypeId::DECIMAL, "-2.9", TypeId::INT), ok("-2"));
    assert_eq!(conv_text(TypeId::DECIMAL, "0.0", TypeId::BOOL), ok("false"));
    assert_eq!(conv_text(TypeId::DECIMAL, "0.1", TypeId::BOOL), ok("true"));
    for refused in ["1e3", "0x1p3", "Inf", "NaN", "1_000", "", "1,5"] {
        assert!(
            conv_text(TypeId::STRING, refused, TypeId::DECIMAL).is_err(),
            "{refused:?}"
        );
    }
}

/// `text` as a `from` value converted to `to`, then to a string: the string, or the error's text.
fn conv_text(from: TypeId, text: &str, to: TypeId) -> Result<String, String> {
    let converted =
        convert(&Val::bytes(from, text.as_bytes().to_vec()), to).map_err(|e| e.to_string())?;
    let binary = marshal(&converted, TypeId::STRING).map_err(|e| e.to_string())?;
    match binary.into_value() {
        Some(Value::Str(s)) => Ok(s),
        other => Err(format!("{other:?}")),
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
fn test_marshal_binary() -> TestResult {
    use ultragraphdb_core_types::marshal_binary;
    assert_eq!(
        marshal_binary(&Val::new(TypeId::INT, Value::Int(-2)))?,
        bs_i64(-2)
    );
    assert_eq!(
        marshal_binary(&Val::new(TypeId::STRING, s("ab")))?,
        b"ab".to_vec()
    );
    // Go's ValueForType of binary holds a *[]byte, on which toBinary's assertion panics.
    assert_eq!(
        marshal_binary(&Val::nil(TypeId::GEO))
            .err()
            .context("expected an error")?
            .to_string(),
        "interface conversion: interface {} is *[]uint8, not []uint8"
    );
    assert_eq!(
        marshal_binary(&Val::new(TypeId::INT, Value::Bool(true)))
            .err()
            .context("expected an error")?
            .to_string(),
        "interface conversion: interface {} is bool, not int64"
    );
    Ok(())
}

#[test]
fn test_convert_date_time_to_float() -> TestResult {
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
            conv(TypeId::DATETIME, bs_time(t)?, TypeId::FLOAT),
            ok(TypeId::FLOAT, Value::Float(want))
        );
    }
    Ok(())
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
fn test_convert_date_time_to_int() -> TestResult {
    let cases = [
        (date(2009, 11, 10, 23, 0, 0, 0), 1257894000),
        (date(1969, 11, 10, 23, 0, 0, 0), -4410000),
        (date(2039, 11, 10, 23, 0, 0, 0), 2204578800),
        (date(1901, 11, 10, 23, 0, 0, 0), -2150326800),
    ];
    for (t, want) in cases {
        assert_eq!(
            conv(TypeId::DATETIME, bs_time(t)?, TypeId::INT),
            ok(TypeId::INT, Value::Int(want))
        );
    }
    Ok(())
}

#[test]
fn test_convert_to_string() -> TestResult {
    let cases = [
        (TypeId::FLOAT, bs_f64(13816.251), "13816.251"),
        (TypeId::INT, bs_i64(-1221), "-1221"),
        (TypeId::BOOL, bs_bool(true), "true"),
        (TypeId::STRING, b"srfrog".to_vec(), "srfrog"),
        (TypeId::PASSWORD, b"password".to_vec(), "password"),
        (
            TypeId::DATETIME,
            bs_time(date(2006, 1, 2, 15, 4, 5, 0))?,
            "2006-01-02T15:04:05Z",
        ),
    ];
    for (tid, data, want) in cases {
        assert_eq!(conv(tid, data, TypeId::STRING), ok(TypeId::STRING, s(want)));
    }
    Ok(())
}

#[test]
fn test_fixed_zone_round_trip() -> TestResult {
    let t = DateTime::new(
        CivilTime::new(2018, 5, 30, 9, 30, 10, 0),
        Zone::Fixed(-6 * 3600),
    );
    let back = conv(TypeId::BINARY, bs_time(t)?, TypeId::DATETIME)
        .context("conv(TypeId::BINARY, bs_time(t), TypeId::DATETIME)")?;
    assert_eq!(back.value(), Some(&Value::DateTime(t)));
    Ok(())
}

#[test]
fn test_marshal_json() -> TestResult {
    use oxsdatatypes::Decimal;
    use ultragraphdb_core_types::marshal_json;
    let j = |v: Val| -> TestResult<String> { Ok(String::from_utf8(marshal_json(&v)?)?) };
    assert_eq!(j(Val::new(TypeId::INT, Value::Int(-3)))?, "-3");
    assert_eq!(j(Val::new(TypeId::FLOAT, Value::Float(1e21)))?, "1e21");
    assert_eq!(j(Val::new(TypeId::FLOAT, Value::Float(0.5)))?, "0.5");
    assert_eq!(j(Val::new(TypeId::BOOL, Value::Bool(true)))?, "true");
    assert_eq!(
        j(Val::new(TypeId::STRING, Value::str("a<b\"\n")))?,
        r#""a<b\"\n""#
    );
    assert_eq!(j(Val::nil(TypeId::DEFAULT))?, r#""""#);
    assert_eq!(
        j(Val::new(
            TypeId::DATETIME,
            Value::DateTime(DateTime::utc(2006, 1, 2, 15, 4, 5, 0))
        ))?,
        r#""2006-01-02T15:04:05Z""#
    );
    let d = "1.5".parse::<Decimal>().ok().context("a decimal")?;
    assert_eq!(j(Val::new(TypeId::DECIMAL, Value::Decimal(d)))?, "1.5");
    let p = Geometry::point(Layout::XY, &[1.0, 2.0])?;
    assert_eq!(
        j(Val::new(TypeId::GEO, Value::Geo(p)))?,
        r#"{"type":"Point","coordinates":[1,2]}"#
    );
    assert_eq!(
        marshal_json(&Val::new(TypeId::FLOAT, Value::Float(f64::NAN)))
            .err()
            .context("expected an error")?
            .to_string(),
        "json: unsupported value: NaN"
    );
    assert_eq!(
        marshal_json(&Val::nil(TypeId::UID))
            .err()
            .context("expected an error")?
            .to_string(),
        "invalid type for MarshalJSON: 7"
    );
    Ok(())
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
            TypeId::DECIMAL,
            Err("interface conversion: interface {} is nil, not decimal"),
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
                .map_err(|e| e.to_string())
                .and_then(|b| String::from_utf8(b).map_err(|e| e.to_string()));
            let want = want.map(String::from).map_err(String::from);
            (got != want).then(|| format!("{id:?}: got {got:?}, Go {want:?}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// One value of each Go type the port's `Value` stands for, as Go's `Val.Value` holds it: `[]uint8`, `string`,
/// `int64`, `float64`, `bool`, `time.Time`, a pointer to each of go-geom's geometry types, `uint64`, `decimal` and
/// `[]float32`.
fn marshal_json_values() -> TestResult<Vec<(&'static str, Value)>> {
    use oxsdatatypes::Decimal;
    let ring = || {
        vec![
            vec![0.0, 0.0],
            vec![1.0, 0.0],
            vec![0.0, 1.0],
            vec![0.0, 0.0],
        ]
    };
    let line = || vec![vec![1.0, 2.0], vec![3.0, 4.0]];
    let point = Geometry::point(Layout::XY, &[1.0, 2.0])?;
    Ok(vec![
        ("Bytes", Value::Bytes(b"ab".to_vec())),
        ("Str", Value::str("a")),
        ("Int", Value::Int(1)),
        ("Float", Value::Float(1.5)),
        ("Bool", Value::Bool(true)),
        (
            "DateTime",
            Value::DateTime(DateTime::utc(2006, 1, 2, 15, 4, 5, 0)),
        ),
        ("Point", Value::Geo(point.clone())),
        (
            "LineString",
            Value::Geo(Geometry::line_string(Layout::XY, &line())?),
        ),
        (
            "Polygon",
            Value::Geo(Geometry::polygon(Layout::XY, &[ring()])?),
        ),
        (
            "MultiPoint",
            Value::Geo(Geometry::multi_point(Layout::XY, &[Some(vec![1.0, 2.0])])?),
        ),
        (
            "MultiLineString",
            Value::Geo(Geometry::multi_line_string(Layout::XY, &[line()])?),
        ),
        (
            "MultiPolygon",
            Value::Geo(Geometry::multi_polygon(Layout::XY, &[vec![ring()]])?),
        ),
        (
            "GeometryCollection",
            Value::Geo(Geometry::collection(vec![point])),
        ),
        ("Uid", Value::Uid(7)),
        (
            "Decimal",
            Value::Decimal("1.5".parse::<Decimal>().ok().context("a decimal")?),
        ),
        ("VFloat", Value::VFloat(vec![1.0, 2.0])),
    ])
}

/// Go v25.5.0's `Val.MarshalJSON` of every type identifier over a value of each Go type the port's `Value` stands
/// for, as Go 1.27 answers `Val{Tid: id, Value: v}.MarshalJSON()`, the values in [`marshal_json_values`]' order.
/// Deviation: where Go panics on its type assertion (`interface conversion: ...`), the port returns the panic's
/// text. Binary, uid, object, vfloat and unknown types are Go's refusal whatever the value; a string or default
/// asserts the value `Safe` returns, which is the value itself when it is not nil.
#[test]
fn test_marshal_json_mismatched_value() -> TestResult {
    use ultragraphdb_core_types::marshal_json;
    let values = marshal_json_values()?;
    let cases: Vec<MismatchCase> = [
        mismatched_default_binary(),
        mismatched_int_float(),
        mismatched_bool_datetime(),
        mismatched_geo_uid(),
        mismatched_password_string(),
        mismatched_object_decimal(),
        mismatched_vfloat_unknown(),
    ]
    .concat();
    let failures: Vec<String> = cases
        .into_iter()
        .flat_map(|(id, wants)| {
            values
                .iter()
                .zip(wants)
                .filter_map(move |((kind, v), want)| {
                    let got = marshal_json(&Val::new(id, v.clone()))
                        .map_err(|e| e.to_string())
                        .and_then(|b| String::from_utf8(b).map_err(|e| e.to_string()));
                    let want = want.map(String::from).map_err(String::from);
                    (got != want).then(|| format!("{id:?} of {kind}: got {got:?}, Go {want:?}"))
                })
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

/// A type, and Go's MarshalJSON of each of `marshal_json_values` under it: the JSON or the error text.
type MismatchCase = (TypeId, [Result<&'static str, &'static str>; 16]);

fn mismatched_default_binary() -> Vec<MismatchCase> {
    vec![
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
                Err("interface conversion: interface {} is decimal, not string"),
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
    ]
}

fn mismatched_int_float() -> Vec<MismatchCase> {
    vec![
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
                Err("interface conversion: interface {} is decimal, not int64"),
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
                Err("interface conversion: interface {} is decimal, not float64"),
                Err("interface conversion: interface {} is []float32, not float64"),
            ],
        ),
    ]
}

fn mismatched_bool_datetime() -> Vec<MismatchCase> {
    vec![
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
                Err("interface conversion: interface {} is decimal, not bool"),
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
                Err("interface conversion: interface {} is decimal, not time.Time"),
                Err("interface conversion: interface {} is []float32, not time.Time"),
            ],
        ),
    ]
}

fn mismatched_geo_uid() -> Vec<MismatchCase> {
    vec![
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
                Err("interface conversion: decimal is not geom.T: missing method Bounds"),
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
    ]
}

fn mismatched_password_string() -> Vec<MismatchCase> {
    vec![
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
                Err("interface conversion: interface {} is decimal, not string"),
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
                Err("interface conversion: interface {} is decimal, not string"),
                Err("interface conversion: interface {} is []float32, not string"),
            ],
        ),
    ]
}

fn mismatched_object_decimal() -> Vec<MismatchCase> {
    vec![
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
            TypeId::DECIMAL,
            [
                Err("interface conversion: interface {} is []uint8, not decimal"),
                Err("interface conversion: interface {} is string, not decimal"),
                Err("interface conversion: interface {} is int64, not decimal"),
                Err("interface conversion: interface {} is float64, not decimal"),
                Err("interface conversion: interface {} is bool, not decimal"),
                Err("interface conversion: interface {} is time.Time, not decimal"),
                Err("interface conversion: interface {} is *geom.Point, not decimal"),
                Err("interface conversion: interface {} is *geom.LineString, not decimal"),
                Err("interface conversion: interface {} is *geom.Polygon, not decimal"),
                Err("interface conversion: interface {} is *geom.MultiPoint, not decimal"),
                Err("interface conversion: interface {} is *geom.MultiLineString, not decimal"),
                Err("interface conversion: interface {} is *geom.MultiPolygon, not decimal"),
                Err("interface conversion: interface {} is *geom.GeometryCollection, not decimal"),
                Err("interface conversion: interface {} is uint64, not decimal"),
                Ok("1.5"),
                Err("interface conversion: interface {} is []float32, not decimal"),
            ],
        ),
    ]
}

fn mismatched_vfloat_unknown() -> Vec<MismatchCase> {
    vec![
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
    ]
}

/// Go v25.5.0's `Val.MarshalJSON` of a geometry with a non-finite coordinate, as Go 1.27 answers it through
/// go-geom's `geojson.Marshal`: the error names the first non-finite coordinate in output order, members of
/// collections in order and nested collections depth first.
#[test]
fn test_marshal_json_geo_non_finite() -> TestResult {
    use ultragraphdb_core_types::marshal_json;
    let cases: Vec<GeoCase> = [
        non_finite_points_lines_and_polygons()?,
        non_finite_multis()?,
        non_finite_multipolygons_collections_and_measures()?,
        non_finite_wider_layouts_and_nesting()?,
    ]
    .concat();
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
    Ok(())
}

/// A case's name, a geometry with a non-finite coordinate, and the coordinate Go's error names.
type GeoCase = (&'static str, Geometry, &'static str);

const INF: f64 = f64::INFINITY;
const NINF: f64 = f64::NEG_INFINITY;
const NAN: f64 = f64::NAN;

fn pt(x: f64, y: f64) -> TestResult<Geometry> {
    Ok(Geometry::point(Layout::XY, &[x, y])?)
}

fn line(c: &[[f64; 2]]) -> Vec<Vec<f64>> {
    c.iter().map(|c| c.to_vec()).collect()
}

fn non_finite_points_lines_and_polygons() -> TestResult<Vec<GeoCase>> {
    Ok(vec![
        ("point +Inf first", pt(INF, 1.0)?, "+Inf"),
        ("point -Inf second", pt(1.0, NINF)?, "-Inf"),
        ("point NaN second", pt(1.0, NAN)?, "NaN"),
        ("point NaN then +Inf", pt(NAN, INF)?, "NaN"),
        ("point -Inf then NaN", pt(NINF, NAN)?, "-Inf"),
        (
            "point xyz +Inf third",
            Geometry::point(Layout::XYZ, &[1.0, 2.0, INF])?,
            "+Inf",
        ),
        (
            "linestring +Inf in second coord",
            Geometry::line_string(Layout::XY, &line(&[[1.0, 2.0], [3.0, INF]]))?,
            "+Inf",
        ),
        (
            "linestring -Inf first then NaN",
            Geometry::line_string(Layout::XY, &line(&[[NINF, 2.0], [NAN, 3.0]]))?,
            "-Inf",
        ),
        (
            "linestring NaN first",
            Geometry::line_string(Layout::XY, &line(&[[NAN, 2.0], [INF, 3.0]]))?,
            "NaN",
        ),
        (
            "polygon NaN then +Inf",
            Geometry::polygon(
                Layout::XY,
                &[line(&[[0.0, 0.0], [1.0, NAN], [INF, 0.0], [0.0, 0.0]])],
            )?,
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
            )?,
            "-Inf",
        ),
        (
            "polygon +Inf first",
            Geometry::polygon(Layout::XY, &[line(&[[INF, 0.0], [NINF, 0.0], [0.0, 0.0]])])?,
            "+Inf",
        ),
    ])
}

fn non_finite_multis() -> TestResult<Vec<GeoCase>> {
    Ok(vec![
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
            )?,
            "NaN",
        ),
        (
            "multipoint -Inf first",
            Geometry::multi_point(Layout::XY, &[Some(vec![NINF, 2.0]), Some(vec![NAN, 1.0])])?,
            "-Inf",
        ),
        (
            "multilinestring -Inf in second line",
            Geometry::multi_line_string(
                Layout::XY,
                &[line(&[[1.0, 2.0]]), line(&[[3.0, NINF], [NAN, 0.0]])],
            )?,
            "-Inf",
        ),
        (
            "multilinestring +Inf first",
            Geometry::multi_line_string(Layout::XY, &[line(&[[INF, 2.0]]), line(&[[3.0, NINF]])])?,
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
            )?,
            "+Inf",
        ),
    ])
}

fn non_finite_multipolygons_collections_and_measures() -> TestResult<Vec<GeoCase>> {
    Ok(vec![
        (
            "multipolygon NaN first",
            Geometry::multi_polygon(
                Layout::XY,
                &[
                    vec![line(&[[NAN, 0.0], [1.0, 0.0], [0.0, 0.0]])],
                    vec![line(&[[0.0, 0.0], [INF, 0.0], [0.0, 0.0]])],
                ],
            )?,
            "NaN",
        ),
        (
            "multipolygon -Inf after empty polygon",
            Geometry::multi_polygon(
                Layout::XY,
                &[vec![], vec![line(&[[0.0, 0.0], [0.0, NINF], [0.0, 0.0]])]],
            )?,
            "-Inf",
        ),
        (
            "collection -Inf in second member",
            Geometry::collection(vec![
                pt(1.0, 2.0)?,
                Geometry::line_string(Layout::XY, &line(&[[1.0, 2.0], [NINF, 0.0]]))?,
                pt(NAN, 0.0)?,
            ]),
            "-Inf",
        ),
        (
            "collection nested NaN before +Inf",
            Geometry::collection(vec![
                Geometry::collection(vec![pt(1.0, 1.0)?]),
                Geometry::collection(vec![pt(NAN, 0.0)?]),
                pt(INF, 0.0)?,
            ]),
            "NaN",
        ),
        (
            "collection +Inf first member",
            Geometry::collection(vec![pt(INF, 0.0)?, pt(NAN, 0.0)?]),
            "+Inf",
        ),
        (
            "point xym NaN m",
            Geometry::point(Layout::XYM, &[1.0, 2.0, NAN])?,
            "NaN",
        ),
        (
            "point xyzm -Inf m",
            Geometry::point(Layout::XYZM, &[1.0, 2.0, 3.0, NINF])?,
            "-Inf",
        ),
    ])
}

fn non_finite_wider_layouts_and_nesting() -> TestResult<Vec<GeoCase>> {
    Ok(vec![
        (
            "linestring xyzm NaN z before +Inf m",
            Geometry::line_string(
                Layout::XYZM,
                &[vec![1.0, 2.0, 3.0, 4.0], vec![5.0, 6.0, NAN, INF]],
            )?,
            "NaN",
        ),
        (
            "multipoint xyz NaN z in second point",
            Geometry::multi_point(
                Layout::XYZ,
                &[Some(vec![1.0, 2.0, 3.0]), Some(vec![4.0, 5.0, NAN])],
            )?,
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
            )?,
            "+Inf",
        ),
        (
            "collection thrice nested -Inf in second ring before +Inf",
            Geometry::collection(vec![
                Geometry::collection(vec![Geometry::collection(vec![Geometry::multi_polygon(
                    Layout::XY,
                    &[vec![
                        line(&[[0.0, 0.0], [1.0, 0.0], [0.0, 0.0]]),
                        line(&[[0.0, 0.0], [0.0, NINF], [NAN, 0.0]]),
                    ]],
                )?])]),
                pt(INF, 0.0)?,
            ]),
            "-Inf",
        ),
        (
            "collection +Inf after empty members",
            Geometry::collection(vec![
                Geometry::collection(vec![]),
                Geometry::point_empty(Layout::XY),
                Geometry::multi_point(Layout::XY, &[])?,
                pt(1.0, INF)?,
            ]),
            "+Inf",
        ),
    ])
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
        ("[0x1p-2]", None),
        ("[1_0]", None),
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
        let got = parse_vfloat(input);
        assert_eq!(
            got.is_ok(),
            want.is_some(),
            "{input:?}: got {got:?}, Go {want:?}"
        );
        match (got, want) {
            (Ok(got), Some(want)) => assert_eq!(bits(&got), bits(want), "{input:?}"),
            (Err(e), _) => assert_eq!(
                e.to_string(),
                format!("cannot convert {} to vfloat", input.trim()),
                "{input:?}"
            ),
            (Ok(_), None) => {}
        }
    }
}

/// Go v25.5.0's `Val.MarshalJSON`, as Go 1.27 writes it: `encoding/json`'s float layout and escapes, a time's
/// RFC 3339 form and its range error, a decimal's text, and the type identifier of every type it refuses.
#[test]
fn test_marshal_json_go_texts() -> TestResult {
    use ultragraphdb_core_types::marshal_json;
    let json = |v: Val| {
        marshal_json(&v)
            .map_err(|e| e.to_string())
            .and_then(|b| String::from_utf8(b).map_err(|e| e.to_string()))
    };
    let cases: Vec<TextCase> = [
        number_and_year_texts(),
        zone_and_string_texts(),
        bool_decimal_and_refused_texts()?,
    ]
    .concat();
    let failures: Vec<String> = cases
        .into_iter()
        .filter_map(|(v, want)| {
            let got = json(v.clone());
            let want = want.map(String::from).map_err(String::from);
            (got != want).then(|| format!("{v:?}: got {got:?}, Go {want:?}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

/// A value and Go's MarshalJSON of it: the JSON or the error text.
type TextCase = (Val, Result<&'static str, &'static str>);

/// Ints, floats and the years outside of RFC 3339's range.
fn number_and_year_texts() -> Vec<TextCase> {
    let float = |f: f64| Val::new(TypeId::FLOAT, Value::Float(f));
    let year_range =
        "json: error calling MarshalJSON for type *time.Time: year outside of range [0,9999]";
    vec![
        (
            Val::new(TypeId::INT, Value::Int(i64::MIN)),
            Ok("-9223372036854775808"),
        ),
        (float(-0.0), Ok("-0")),
        (float(1e-7), Ok("1e-7")),
        (float(1e-6), Ok("0.000001")),
        (float(1e20), Ok("100000000000000000000")),
        (float(1e21), Ok("1e21")),
        (float(123456789.0), Ok("123456789")),
        (float(f64::MAX), Ok("1.7976931348623157e308")),
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
    ]
}

/// Times in a zone, and strings with the characters Go escapes.
fn zone_and_string_texts() -> Vec<TextCase> {
    vec![
        (
            Val::new(
                TypeId::DATETIME,
                Value::DateTime(DateTime::new(
                    CivilTime::new(2018, 5, 30, 9, 30, 10, 500_000_000),
                    Zone::Fixed(-6 * 3600),
                )),
            ),
            Ok(r#""2018-05-30T09:30:10.5-06:00""#),
        ),
        (Val::new(TypeId::PASSWORD, s("p\"w<")), Ok(r#""p\"w<""#)),
        (Val::new(TypeId::STRING, s("")), Ok(r#""""#)),
        (
            Val::new(TypeId::DEFAULT, s("日本\u{2028}")),
            Ok("\"日本\u{2028}\""),
        ),
        (
            Val::new(
                TypeId::DATETIME,
                Value::DateTime(DateTime::new(
                    CivilTime::new(2018, 1, 1, 0, 0, 0, 0),
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
                Value::DateTime(DateTime::new(
                    CivilTime::new(2018, 1, 1, 0, 0, 0, 0),
                    Zone::Fixed(30),
                )),
            ),
            Ok(r#""2018-01-01T00:00:00+00:00""#),
        ),
    ]
}

/// A bool, decimals, and the types MarshalJSON refuses.
fn bool_decimal_and_refused_texts() -> TestResult<Vec<TextCase>> {
    use oxsdatatypes::Decimal;
    let big = |s: &str| -> TestResult<Val> {
        Ok(Val::new(
            TypeId::DECIMAL,
            Value::Decimal(s.parse::<Decimal>().ok().context("a decimal")?),
        ))
    };
    Ok(vec![
        (Val::new(TypeId::BOOL, Value::Bool(false)), Ok("false")),
        (big("100.50")?, Ok("100.5")),
        (big("-0.125")?, Ok("-0.125")),
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
    ])
}

#[test]
fn test_convert_null_geojson_to_geo_is_nil() -> TestResult {
    let got = conv(TypeId::STRING, b"null".to_vec(), TypeId::GEO).context("null as geo")?;
    assert_eq!(got, Val::nil(TypeId::GEO));
    Ok(())
}

#[test]
fn test_convert_geo_without_geojson_to_string_is_nil() -> TestResult {
    let g = Geometry::point(Layout::XY, &[f64::NAN, 0.0])?;
    assert_eq!(
        g.to_geojson(),
        None,
        "a non-finite coordinate has no GeoJSON"
    );
    let got = conv(TypeId::GEO, g.marshal_wkb()?, TypeId::STRING).context("geo as a string")?;
    assert_eq!(got, Val::nil(TypeId::STRING));
    Ok(())
}

#[test]
fn test_marshal_a_value_of_another_type_is_refused_naming_both() -> TestResult {
    let int = Value::Int(1);
    for (tid, want) in [
        (TypeId::STRING, "string"),
        (TypeId::PASSWORD, "string"),
        (TypeId::VFLOAT, "[]float32"),
    ] {
        let err = marshal(&Val::new(tid, int.clone()), TypeId::BINARY)
            .err()
            .context("a mismatched value is refused")?
            .to_string();
        assert!(
            err.ends_with(&format!("is {}, not {want}", int.go_type())),
            "{tid:?}: {err}"
        );
    }
    Ok(())
}

#[test]
fn test_marshal_to_a_type_without_that_form_is_refused() -> TestResult {
    for (from, to) in [
        (Val::new(TypeId::STRING, s("a")), TypeId::INT),
        (Val::new(TypeId::PASSWORD, s("a")), TypeId::INT),
        (
            Val::new(TypeId::VFLOAT, Value::VFloat(vec![1.0])),
            TypeId::INT,
        ),
        (Val::new(TypeId::UID, Value::Uid(1)), TypeId::BINARY),
    ] {
        let err = marshal(&from, to)
            .err()
            .context("a marshal without that form is refused")?
            .to_string();
        assert_eq!(
            err,
            format!("Cannot convert {} to type {}", from.tid().name(), to.name()),
        );
    }
    Ok(())
}
