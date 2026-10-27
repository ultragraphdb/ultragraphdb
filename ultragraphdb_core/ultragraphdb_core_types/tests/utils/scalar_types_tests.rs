/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/scalar_types_test.go. Changed for ultragraphdb: translated to Rust; plus the
 * golden ParseTime vectors (tools/golden/types.go, types_parse_time).
 * Copyright (c) 2026. Marvin Hansen.
 */

use serde::Deserialize;
use ultragraphdb_core_types::{
    BIG_FLOAT_PRECISION, DateTime, Geometry, TYPE_NAMES, TypeId, Value, Zone, good_time_zone,
    parse_time, value_for_type,
};

#[allow(clippy::too_many_arguments)]
fn fixed(y: i64, mo: i64, d: i64, h: i64, mi: i64, s: i64, ns: i64, offset: i32) -> DateTime {
    DateTime::new(y, mo, d, h, mi, s, ns, Zone::Fixed(offset))
}

/// Go's test expects UTC for `-00:00` only because it sets `time.Local = time.UTC`; Dgraph never does, and then
/// a numeric zero offset is a fixed zone (binary offset 0, not UTC's -1).
fn dates_with_tz() -> Vec<(&'static str, DateTime)> {
    vec![
        (
            "2018-10-28T04:00:10Z",
            DateTime::utc(2018, 10, 28, 4, 0, 10, 0),
        ),
        (
            "2018-10-28T04:00:10-00:00",
            fixed(2018, 10, 28, 4, 0, 10, 0, 0),
        ),
        (
            "2018-05-30T09:30:10.5Z",
            DateTime::utc(2018, 5, 30, 9, 30, 10, 500000000),
        ),
        (
            "2018-05-30T09:30:10.5-00:00",
            fixed(2018, 5, 30, 9, 30, 10, 500000000, 0),
        ),
        (
            "2018-05-30T09:30:10-06:00",
            fixed(2018, 5, 30, 9, 30, 10, 0, -6 * 3600),
        ),
        (
            "2018-05-28T14:41:57+23:00",
            fixed(2018, 5, 28, 14, 41, 57, 0, 23 * 3600),
        ),
        (
            "2018-05-28T14:41:57-23:40",
            fixed(2018, 5, 28, 14, 41, 57, 0, -23 * 3600 - 40 * 60),
        ),
        (
            "2018-05-28T14:41:57+23:59",
            fixed(2018, 5, 28, 14, 41, 57, 0, 23 * 3600 + 59 * 60),
        ),
    ]
}

const DATES_WITH_INVALID_TZ: [&str; 4] = [
    "2018-05-28T14:41:57+25:00",
    "2018-05-28T14:41:57+30:00",
    "2018-05-28T14:41:57-25:01",
    "2018-05-28T14:41:57-30:00",
];

fn dates_without_tz() -> Vec<(&'static str, DateTime)> {
    vec![
        (
            "2018-10-28T04:00:10",
            DateTime::utc(2018, 10, 28, 4, 0, 10, 0),
        ),
        (
            "2018-05-30T09:30:10.5",
            DateTime::utc(2018, 5, 30, 9, 30, 10, 500000000),
        ),
        ("2018", DateTime::utc(2018, 1, 1, 0, 0, 0, 0)),
        ("2018-01", DateTime::utc(2018, 1, 1, 0, 0, 0, 0)),
        ("2018-01-01", DateTime::utc(2018, 1, 1, 0, 0, 0, 0)),
    ]
}

const INVALID_DATES: [&str; 22] = [
    "abcd",
    "12345",
    "123456",
    "1234567",
    "12345678",
    "123456789",
    "1234567891",
    "11111111111111111Z",
    "111111111111111:11",
    "1111-11-11T11:11111111:1",
    "1111-11-11T11:11:1111:11",
    "18-10-28T04:00:10Z",
    "318-10-28T04:00:10",
    "2018-110-28T04:00:10",
    "20181-4-28T25:00:10",
    "2018-10-218T04:00:10",
    "2018-14-28T25:00:10",
    "2018-142-8T25:00:10",
    "2018-05-33T09:65:10.5",
    "201",
    "2018-011",
    "2018-01-011",
];

#[test]
fn test_type_for_name() {
    for (name, tid) in TYPE_NAMES {
        assert_eq!(TypeId::for_name(name), Some(tid), "{name}");
        assert_eq!(tid.name(), name);
    }
    assert_eq!(TypeId::for_name("--invalid--"), None);
}

/// Go's `ValueForType` returns pointers to each type's zero value; this port returns the values, a geometry's
/// nil `geom.T` as an empty collection. A type without a zero (object, and any unknown) is a nil default.
#[test]
fn test_value_for_type() {
    let cases = [
        (TypeId::DEFAULT, Some(Value::str(""))),
        (TypeId::BINARY, Some(Value::Bytes(Vec::new()))),
        (TypeId::INT, Some(Value::Int(0))),
        (TypeId::FLOAT, Some(Value::Float(0.0))),
        (TypeId::BOOL, Some(Value::Bool(false))),
        (TypeId::DATETIME, Some(Value::DateTime(DateTime::zero()))),
        (
            TypeId::GEO,
            Some(Value::Geo(Geometry::collection(Vec::new()))),
        ),
        (TypeId::UID, Some(Value::Uid(0))),
        (TypeId::PASSWORD, Some(Value::str(""))),
        (TypeId::STRING, Some(Value::str(""))),
        (TypeId::VFLOAT, Some(Value::VFloat(Vec::new()))),
    ];
    for (tid, want) in cases {
        let val = value_for_type(tid);
        assert_eq!(val.tid(), tid, "{}", tid.name());
        assert_eq!(val.value(), want.as_ref(), "{}", tid.name());
    }
    let big = value_for_type(TypeId::BIGFLOAT);
    assert_eq!(big.tid(), TypeId::BIGFLOAT);
    match big.value() {
        Some(Value::BigFloat(b)) => {
            assert!(b.is_zero() && !b.is_neg());
            assert_eq!(b.prec(), 200);
            assert_eq!(BIG_FLOAT_PRECISION, 200);
        }
        other => panic!("{other:?}"),
    }
    for tid in [TypeId::OBJECT, TypeId::UNDEFINED, TypeId::new(99)] {
        let val = value_for_type(tid);
        assert_eq!(val.tid(), TypeId::DEFAULT, "{tid:?}");
        assert!(val.value().is_none(), "{tid:?}");
    }
}

#[test]
fn test_parse_time_without_tz() {
    for (input, want) in dates_without_tz() {
        assert_eq!(parse_time(input).unwrap(), want, "{input}");
    }
}

#[test]
fn test_parse_time_with_tz() {
    for (input, want) in dates_with_tz() {
        assert_eq!(parse_time(input).unwrap(), want, "{input}");
    }
}

#[test]
fn test_parse_time_rejection() {
    // Go's time.ParseError always opens with the quoted input; the full texts are in the golden vectors.
    for input in INVALID_DATES {
        let err = parse_time(input).unwrap_err().to_string();
        assert!(
            err.starts_with(&format!("parsing time \"{input}\"")),
            "{input}: {err}"
        );
    }
}

#[test]
fn test_parse_time_non_rfc3339() {
    for input in DATES_WITH_INVALID_TZ {
        let err = parse_time(input).unwrap_err().to_string();
        assert!(
            err.contains("time zone offset hour out of range"),
            "{input}: {err}"
        );
    }
}

#[test]
fn test_good_time_zone() {
    // Go's GoodTimeZone: RFC 3339's offsets, ±23:59, both ends inclusive.
    for (offset, want) in [
        (0, true),
        (86340, true),
        (86341, false),
        (-86340, true),
        (-86341, false),
        (86400, false),
        (i64::MIN, false),
        (i64::MAX, false),
    ] {
        assert_eq!(good_time_zone(offset), want, "{offset}");
    }
}

#[derive(Deserialize)]
struct ParseTimeCase {
    #[serde(rename = "in")]
    input: String,
    #[serde(default)]
    err: String,
    #[serde(default)]
    bin: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    unix: i64,
    #[serde(default)]
    nsec: u32,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|c| format!("{c:02x}")).collect()
}

#[test]
fn test_golden_parse_time() {
    let path = ultragraphdb_tool_bdd::testdata_dir().join("golden/types_parse_time.json");
    let golden = ultragraphdb_tool_bdd::load_golden::<ParseTimeCase>(&path);
    for c in golden.cases() {
        let got = parse_time(&c.input);
        if !c.err.is_empty() {
            assert_eq!(got.unwrap_err().to_string(), c.err, "{}", c.input);
            continue;
        }
        let t = got.unwrap_or_else(|e| panic!("{}: {e}", c.input));
        let bin = t
            .marshal_binary()
            .map(|b| hex(&b))
            .unwrap_or_else(|e| format!("error: {e}"));
        assert_eq!(bin, c.bin, "{}", c.input);
        let text = t.marshal_text().unwrap_or_else(|e| format!("error: {e}"));
        assert_eq!(text, c.text, "{}", c.input);
        assert_eq!((t.unix(), t.nanosecond()), (c.unix, c.nsec), "{}", c.input);
    }
}
