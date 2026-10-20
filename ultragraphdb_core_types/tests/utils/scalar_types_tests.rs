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
    DateTime, TYPE_NAMES, TypeId, Zone, good_time_zone, parse_time, value_for_type,
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

#[test]
fn test_value_for_type() {
    for (name, tid) in TYPE_NAMES {
        let val = value_for_type(tid);
        assert_eq!(val.tid(), tid, "{name}");
        assert!(val.value().is_some(), "{name}");
    }
    let val = value_for_type(TypeId::UNDEFINED);
    assert_eq!(val.tid(), TypeId::DEFAULT);
    assert!(val.value().is_none());
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
    for input in INVALID_DATES {
        assert!(parse_time(input).is_err(), "{input}");
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
    assert!(good_time_zone(23 * 3600 + 59 * 60));
    assert!(good_time_zone(-(23 * 3600 + 59 * 60)));
    assert!(!good_time_zone(24 * 3600));
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
    let path = ultragraphdb_test_bdd::testdata_dir("ultragraphdb_core_types")
        .join("golden/types_parse_time.json");
    let golden = ultragraphdb_test_bdd::load_golden::<ParseTimeCase>(&path);
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
