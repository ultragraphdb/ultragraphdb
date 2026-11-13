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
    CivilTime, DateTime, Geometry, TYPE_NAMES, TypeId, Value, Zone, good_time_zone, parse_time,
    value_for_type,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

fn fixed(civil: CivilTime, offset: i32) -> DateTime {
    DateTime::new(civil, Zone::Fixed(offset))
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
            fixed(CivilTime::new(2018, 10, 28, 4, 0, 10, 0), 0),
        ),
        (
            "2018-05-30T09:30:10.5Z",
            DateTime::utc(2018, 5, 30, 9, 30, 10, 500000000),
        ),
        (
            "2018-05-30T09:30:10.5-00:00",
            fixed(CivilTime::new(2018, 5, 30, 9, 30, 10, 500000000), 0),
        ),
        (
            "2018-05-30T09:30:10-06:00",
            fixed(CivilTime::new(2018, 5, 30, 9, 30, 10, 0), -6 * 3600),
        ),
        (
            "2018-05-28T14:41:57+23:00",
            fixed(CivilTime::new(2018, 5, 28, 14, 41, 57, 0), 23 * 3600),
        ),
        (
            "2018-05-28T14:41:57-23:40",
            fixed(
                CivilTime::new(2018, 5, 28, 14, 41, 57, 0),
                -23 * 3600 - 40 * 60,
            ),
        ),
        (
            "2018-05-28T14:41:57+23:59",
            fixed(
                CivilTime::new(2018, 5, 28, 14, 41, 57, 0),
                23 * 3600 + 59 * 60,
            ),
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

const INVALID_DATES: [&str; 27] = [
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
    "2020-01-ab",
    "2020-01-02Tab:04:05",
    "2020-01-02T15:04:ab",
    "2020-01-02 15:04:05 +ab00 MST",
    "2020-01-02 15:04:05 x0700 MST",
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
fn test_value_for_type() -> TestResult {
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
    let decimal = value_for_type(TypeId::DECIMAL);
    assert_eq!(decimal.tid(), TypeId::DECIMAL);
    assert_eq!(
        decimal.value(),
        Some(&Value::Decimal(oxsdatatypes::Decimal::from(0_i64)))
    );
    for tid in [TypeId::OBJECT, TypeId::UNDEFINED, TypeId::new(99)] {
        let val = value_for_type(tid);
        assert_eq!(val.tid(), TypeId::DEFAULT, "{tid:?}");
        assert!(val.value().is_none(), "{tid:?}");
    }
    Ok(())
}

#[test]
fn test_parse_time_without_tz() -> TestResult {
    for (input, want) in dates_without_tz() {
        assert_eq!(parse_time(input)?, want, "{input}");
    }
    Ok(())
}

#[test]
fn test_parse_time_with_tz() -> TestResult {
    for (input, want) in dates_with_tz() {
        assert_eq!(parse_time(input)?, want, "{input}");
    }
    Ok(())
}

#[test]
fn test_parse_time_rejection() -> TestResult {
    // Go's time.ParseError always opens with the quoted input; the full texts are in the golden vectors.
    for input in INVALID_DATES {
        let err = parse_time(input)
            .err()
            .context("expected an error")?
            .to_string();
        assert!(
            err.starts_with(&format!("parsing time \"{input}\"")),
            "{input}: {err}"
        );
    }
    Ok(())
}

#[test]
fn test_parse_time_non_rfc3339() -> TestResult {
    for input in DATES_WITH_INVALID_TZ {
        let err = parse_time(input)
            .err()
            .context("expected an error")?
            .to_string();
        assert!(
            err.contains("time zone offset hour out of range"),
            "{input}: {err}"
        );
    }
    Ok(())
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
fn test_golden_parse_time() -> TestResult {
    let path = ultragraphdb_tool_bdd::testdata_dir()?.join("golden/types_parse_time.json");
    let golden = ultragraphdb_tool_bdd::load_golden::<ParseTimeCase>(&path)?;
    for c in golden.cases() {
        let got = parse_time(&c.input);
        if !c.err.is_empty() {
            assert_eq!(
                got.err().context("expected an error")?.to_string(),
                c.err,
                "{}",
                c.input
            );
            continue;
        }
        let t = got
            .map_err(|e| format!("{}: {e}", c.input))
            .context("parsing a golden time")?;
        let bin = t
            .marshal_binary()
            .map(|b| hex(&b))
            .unwrap_or_else(|e| format!("error: {e}"));
        assert_eq!(bin, c.bin, "{}", c.input);
        let text = t.marshal_text().unwrap_or_else(|e| format!("error: {e}"));
        assert_eq!(text, c.text, "{}", c.input);
        assert_eq!((t.unix(), t.nanosecond()), (c.unix, c.nsec), "{}", c.input);
    }
    Ok(())
}
