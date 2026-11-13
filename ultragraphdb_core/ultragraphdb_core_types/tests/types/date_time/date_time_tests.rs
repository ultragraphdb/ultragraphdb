/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A datetime's instants, binary form and text. The expected values are what Go 1.27's `time` gives for the same
//! inputs, whose binary layout stored values hold; where a form refuses, the tables say so with `Err`.

use ultragraphdb_core_types::{CivilTime, DateTime, Zone};
use ultragraphdb_tool_bdd::TestResult;

const UTC: Option<i32> = None;

fn zone(offset: Option<i32>) -> Zone {
    offset.map_or(Zone::Utc, Zone::Fixed)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|c| format!("{c:02x}")).collect()
}

/// `time.Date(year, month, day, hour, min, sec, nsec, zone)` and what Go makes of it.
struct DateCase {
    date: [i64; 7],
    offset: Option<i32>,
    unix: i64,
    unix_nano: i64,
    nsec: u32,
    /// `MarshalBinary` in hex, or its error.
    binary: Result<&'static str, &'static str>,
    /// `UnmarshalBinary` of that: Unix seconds, nanoseconds, the zone's offset, and whether it is UTC.
    back: Option<(i64, u32, i32, bool)>,
    /// `MarshalText`, or its error.
    text: Result<&'static str, &'static str>,
}

// Why a form refuses a time; the tables name the reason, and the tests check that it refuses.
const ZONE_ERR: &str = "a zone offset the binary form cannot hold";
const YEAR_ERR: &str = "a year outside 0 to 9999";
const HOUR_ERR: &str = "a zone offset past 23:59";

#[rustfmt::skip]
const DATES: [DateCase; 36] = [
    DateCase { date: [2018, 10, 3, 8, 5, 1, 0], offset: UTC, unix: 1538553901, unix_nano: 1538553901000000000, nsec: 0,
        binary: Ok("010000000ed3466f2d00000000ffff"), back: Some((1538553901, 0, 0, true)), text: Ok("2018-10-03T08:05:01Z") },
    DateCase { date: [2018, 10, 3, 8, 5, 1, 0], offset: Some(0), unix: 1538553901, unix_nano: 1538553901000000000, nsec: 0,
        binary: Ok("010000000ed3466f2d000000000000"), back: Some((1538553901, 0, 0, false)), text: Ok("2018-10-03T08:05:01Z") },
    DateCase { date: [2018, 5, 28, 14, 41, 57, 0], offset: Some(19815), unix: 1527498702, unix_nano: 1527498702000000000, nsec: 0,
        binary: Ok("020000000ed29dbece00000000014a0f"), back: Some((1527498702, 0, 19815, false)), text: Ok("2018-05-28T14:41:57+05:30") },
    // Go writes the negative leftover seconds as a signed byte and reads them back as -15.
    DateCase { date: [2018, 5, 28, 14, 41, 57, 0], offset: Some(-19815), unix: 1527538332, unix_nano: 1527538332000000000, nsec: 0,
        binary: Ok("020000000ed29e599c00000000feb6f1"), back: Some((1527538332, 0, -19815, false)), text: Ok("2018-05-28T14:41:57-05:30") },
    // Go's TestMarshalBinaryVersion2Bugfix zone, -05:30:45.
    DateCase { date: [2018, 5, 28, 14, 41, 57, 0], offset: Some(-19845), unix: 1527538362, unix_nano: 1527538362000000000, nsec: 0,
        binary: Ok("020000000ed29e59ba00000000feb6d3"), back: Some((1527538362, 0, -19845, false)), text: Ok("2018-05-28T14:41:57-05:30") },
    DateCase { date: [2018, 5, 28, 14, 41, 57, 0], offset: Some(-3599), unix: 1527522116, unix_nano: 1527522116000000000, nsec: 0,
        binary: Ok("020000000ed29e1a4400000000ffc5c5"), back: Some((1527522116, 0, -3599, false)), text: Ok("2018-05-28T14:41:57-00:59") },
    DateCase { date: [2018, 5, 28, 14, 41, 57, 0], offset: Some(-60), unix: 1527518577, unix_nano: 1527518577000000000, nsec: 0,
        binary: Err(ZONE_ERR), back: None, text: Ok("2018-05-28T14:41:57-00:01") },
    DateCase { date: [2018, 5, 28, 14, 41, 57, 0], offset: Some(-59), unix: 1527518576, unix_nano: 1527518576000000000, nsec: 0,
        binary: Ok("020000000ed29e0c70000000000000c5"), back: Some((1527518576, 0, -59, false)), text: Ok("2018-05-28T14:41:57+00:00") },
    DateCase { date: [2018, 5, 28, 14, 41, 57, 0], offset: Some(30), unix: 1527518487, unix_nano: 1527518487000000000, nsec: 0,
        binary: Ok("020000000ed29e0c170000000000001e"), back: Some((1527518487, 0, 30, false)), text: Ok("2018-05-28T14:41:57+00:00") },
    DateCase { date: [2018, 5, 28, 14, 41, 57, 0], offset: Some(3599), unix: 1527514918, unix_nano: 1527514918000000000, nsec: 0,
        binary: Ok("020000000ed29dfe2600000000003b3b"), back: Some((1527514918, 0, 3599, false)), text: Ok("2018-05-28T14:41:57+00:59") },
    DateCase { date: [2018, 5, 28, 14, 41, 57, 0], offset: Some(86400), unix: 1527432117, unix_nano: 1527432117000000000, nsec: 0,
        binary: Ok("010000000ed29cbab50000000005a0"), back: Some((1527432117, 0, 86400, false)), text: Err(HOUR_ERR) },
    DateCase { date: [2018, 5, 28, 14, 41, 57, 0], offset: Some(-86400), unix: 1527604917, unix_nano: 1527604917000000000, nsec: 0,
        binary: Ok("010000000ed29f5db500000000fa60"), back: Some((1527604917, 0, -86400, false)), text: Err(HOUR_ERR) },
    DateCase { date: [2018, 5, 28, 14, 41, 57, 0], offset: Some(86340), unix: 1527432177, unix_nano: 1527432177000000000, nsec: 0,
        binary: Ok("010000000ed29cbaf100000000059f"), back: Some((1527432177, 0, 86340, false)), text: Ok("2018-05-28T14:41:57+23:59") },
    DateCase { date: [2018, 5, 28, 14, 41, 57, 0], offset: Some(32767 * 60), unix: 1525552497, unix_nano: 1525552497000000000, nsec: 0,
        binary: Ok("010000000ed2800c71000000007fff"), back: Some((1525552497, 0, 1966020, false)), text: Err(HOUR_ERR) },
    DateCase { date: [2018, 5, 28, 14, 41, 57, 0], offset: Some(32768 * 60), unix: 1525552437, unix_nano: 1525552437000000000, nsec: 0,
        binary: Err(ZONE_ERR), back: None, text: Err(HOUR_ERR) },
    DateCase { date: [2018, 5, 28, 14, 41, 57, 0], offset: Some(-32768 * 60), unix: 1529484597, unix_nano: 1529484597000000000, nsec: 0,
        binary: Ok("010000000ed2bc0c35000000008000"), back: Some((1529484597, 0, -1966080, false)), text: Err(HOUR_ERR) },
    DateCase { date: [2018, 5, 28, 14, 41, 57, 0], offset: Some(-32769 * 60), unix: 1529484657, unix_nano: 1529484657000000000, nsec: 0,
        binary: Err(ZONE_ERR), back: None, text: Err(HOUR_ERR) },
    DateCase { date: [2006, 1, 2, 15, 4, 5, 120000000], offset: Some(-25200), unix: 1136239445, unix_nano: 1136239445120000000, nsec: 120000000,
        binary: Ok("010000000ebb4b9a5507270e00fe5c"), back: Some((1136239445, 120000000, -25200, false)), text: Ok("2006-01-02T15:04:05.12-07:00") },
    DateCase { date: [2006, 1, 2, 15, 4, 5, 1], offset: Some(3600), unix: 1136210645, unix_nano: 1136210645000000001, nsec: 1,
        binary: Ok("010000000ebb4b29d500000001003c"), back: Some((1136210645, 1, 3600, false)), text: Ok("2006-01-02T15:04:05.000000001+01:00") },
    DateCase { date: [2006, 1, 2, 15, 4, 5, 999999999], offset: UTC, unix: 1136214245, unix_nano: 1136214245999999999, nsec: 999999999,
        binary: Ok("010000000ebb4b37e53b9ac9ffffff"), back: Some((1136214245, 999999999, 0, true)), text: Ok("2006-01-02T15:04:05.999999999Z") },
    DateCase { date: [1, 1, 1, 0, 0, 0, 0], offset: UTC, unix: -62135596800, unix_nano: -6795364578871345152, nsec: 0,
        binary: Ok("01000000000000000000000000ffff"), back: Some((-62135596800, 0, 0, true)), text: Ok("0001-01-01T00:00:00Z") },
    DateCase { date: [0, 1, 1, 0, 0, 0, 0], offset: UTC, unix: -62167219200, unix_nano: -6826986978871345152, nsec: 0,
        binary: Ok("01fffffffffe1d7b0000000000ffff"), back: Some((-62167219200, 0, 0, true)), text: Ok("0000-01-01T00:00:00Z") },
    DateCase { date: [-1, 12, 31, 23, 59, 59, 0], offset: UTC, unix: -62167219201, unix_nano: -6826986979871345152, nsec: 0,
        binary: Ok("01fffffffffe1d7aff00000000ffff"), back: Some((-62167219201, 0, 0, true)), text: Err(YEAR_ERR) },
    DateCase { date: [9999, 12, 31, 23, 59, 59, 999999999], offset: UTC, unix: 253402300799, unix_nano: -4852116231933722625, nsec: 999999999,
        binary: Ok("01000000497786387f3b9ac9ffffff"), back: Some((253402300799, 999999999, 0, true)), text: Ok("9999-12-31T23:59:59.999999999Z") },
    DateCase { date: [10000, 1, 1, 0, 0, 0, 0], offset: UTC, unix: 253402300800, unix_nano: -4852116231933722624, nsec: 0,
        binary: Ok("01000000497786388000000000ffff"), back: Some((253402300800, 0, 0, true)), text: Err(YEAR_ERR) },
    DateCase { date: [2000, 2, 29, 12, 0, 0, 0], offset: UTC, unix: 951825600, unix_nano: 951825600000000000, nsec: 0,
        binary: Ok("010000000eb04dabc000000000ffff"), back: Some((951825600, 0, 0, true)), text: Ok("2000-02-29T12:00:00Z") },
    // 1900 is not a leap year: February 29 is March 1.
    DateCase { date: [1900, 2, 29, 12, 0, 0, 0], offset: UTC, unix: -2203848000, unix_nano: -2203848000000000000, nsec: 0,
        binary: Ok("010000000df435e9c000000000ffff"), back: Some((-2203848000, 0, 0, true)), text: Ok("1900-03-01T12:00:00Z") },
    DateCase { date: [2024, 13, 1, 0, 0, 0, 0], offset: UTC, unix: 1735689600, unix_nano: 1735689600000000000, nsec: 0,
        binary: Ok("010000000edf067c8000000000ffff"), back: Some((1735689600, 0, 0, true)), text: Ok("2025-01-01T00:00:00Z") },
    DateCase { date: [2024, 0, 0, 0, 0, 0, 0], offset: UTC, unix: 1701302400, unix_nano: 1701302400000000000, nsec: 0,
        binary: Ok("010000000edcf9c78000000000ffff"), back: Some((1701302400, 0, 0, true)), text: Ok("2023-11-30T00:00:00Z") },
    DateCase { date: [2024, 3, 1, -1, 0, 0, 0], offset: UTC, unix: 1709247600, unix_nano: 1709247600000000000, nsec: 0,
        binary: Ok("010000000edd73037000000000ffff"), back: Some((1709247600, 0, 0, true)), text: Ok("2024-02-29T23:00:00Z") },
    DateCase { date: [2024, 1, 1, 0, 0, 0, -1], offset: UTC, unix: 1704067199, unix_nano: 1704067199999999999, nsec: 999999999,
        binary: Ok("010000000edd23f77f3b9ac9ffffff"), back: Some((1704067199, 999999999, 0, true)), text: Ok("2023-12-31T23:59:59.999999999Z") },
    DateCase { date: [2024, 1, 1, 0, 0, 0, 1500000000], offset: UTC, unix: 1704067201, unix_nano: 1704067201500000000, nsec: 500000000,
        binary: Ok("010000000edd23f7811dcd6500ffff"), back: Some((1704067201, 500000000, 0, true)), text: Ok("2024-01-01T00:00:01.5Z") },
    DateCase { date: [2024, 1, 1, 24, 60, 60, 0], offset: UTC, unix: 1704157260, unix_nano: 1704157260000000000, nsec: 0,
        binary: Ok("010000000edd25574c00000000ffff"), back: Some((1704157260, 0, 0, true)), text: Ok("2024-01-02T01:01:00Z") },
    DateCase { date: [1969, 12, 31, 23, 59, 59, 0], offset: UTC, unix: -1, unix_nano: -1000000000, nsec: 0,
        binary: Ok("010000000e7791f6ff00000000ffff"), back: Some((-1, 0, 0, true)), text: Ok("1969-12-31T23:59:59Z") },
    // UnixNano wraps outside 1678 to 2262.
    DateCase { date: [1677, 1, 1, 0, 0, 0, 0], offset: UTC, unix: -9246096000, unix_nano: 9200648073709551616, nsec: 0,
        binary: Ok("010000000c5075bc8000000000ffff"), back: Some((-9246096000, 0, 0, true)), text: Ok("1677-01-01T00:00:00Z") },
    DateCase { date: [2263, 1, 1, 0, 0, 0, 0], offset: UTC, unix: 9246182400, unix_nano: -9200561673709551616, nsec: 0,
        binary: Ok("01000000109eaf830000000000ffff"), back: Some((9246182400, 0, 0, true)), text: Ok("2263-01-01T00:00:00Z") },
];

#[test]
fn test_dates_as_go_computes_and_encodes_them() -> TestResult {
    for c in &DATES {
        let [y, mo, d, h, mi, s, ns] = c.date;
        let t = DateTime::new(CivilTime::new(y, mo, d, h, mi, s, ns), zone(c.offset));
        let at = format!("{:?} {:?}", c.date, c.offset);
        assert_eq!(
            (t.unix(), t.unix_nano(), t.nanosecond()),
            (c.unix, c.unix_nano, c.nsec),
            "{at}"
        );
        assert_eq!(t.zone(), zone(c.offset), "{at}");
        assert_eq!(t.offset(), c.offset.unwrap_or(0), "{at}");
        let bin = t.marshal_binary();
        assert_eq!(
            bin.as_deref().map(hex).ok(),
            c.binary.map(str::to_string).ok(),
            "{at}: {bin:?}"
        );
        if let (Ok(b), Some((unix, nsec, offset, utc))) = (&bin, c.back) {
            let u = DateTime::unmarshal_binary(b)?;
            assert_eq!(
                (u.unix(), u.nanosecond(), u.offset(), u.zone() == Zone::Utc),
                (unix, nsec, offset, utc),
                "{at}"
            );
            assert!(u.equal(&t), "{at}");
        }
        let text = t.marshal_text();
        assert_eq!(
            text.as_ref().ok().cloned(),
            c.text.map(str::to_string).ok(),
            "{at}: {text:?}"
        );
        if c.offset.is_none() {
            assert_eq!(t, DateTime::utc(y, mo, d, h, mi, s, ns), "{at}");
        }
    }
    Ok(())
}

/// The version, seconds since year 1, nanoseconds and offset in minutes of Go's binary layout, with `extra` after.
fn binary(version: u8, sec: i64, nsec: u32, offset_min: i16, extra: &[u8]) -> Vec<u8> {
    let mut b = vec![version];
    b.extend(sec.to_be_bytes());
    b.extend(nsec.to_be_bytes());
    b.extend(offset_min.to_be_bytes());
    b.extend(extra);
    b
}

#[test]
fn test_unmarshal_binary_reads_every_offset() -> TestResult {
    // The internal seconds of the Unix epoch.
    const EPOCH: i64 = 62135596800;
    // (input, Unix seconds, nanoseconds, offset, UTC).
    let cases = [
        (binary(1, EPOCH, 0, -1, &[]), 0, 0, 0, true),
        (
            binary(1, EPOCH, 999_999_999, 60, &[]),
            0,
            999_999_999,
            3600,
            false,
        ),
        // Version 2's leftover seconds are a signed byte.
        (binary(2, EPOCH, 0, 0, &[0xff]), 0, 0, -1, false),
        (binary(2, EPOCH, 0, 0, &[0x80]), 0, 0, -128, false),
        (binary(2, EPOCH, 0, 0, &[0x7f]), 0, 0, 127, false),
        (binary(2, EPOCH, 0, 0, &[0xc5]), 0, 0, -59, false),
        // 0 minutes and -60 seconds is the offset of minus one minute, which is UTC.
        (binary(2, EPOCH, 0, 0, &[0xc4]), 0, 0, 0, true),
        (binary(2, EPOCH, 0, 1, &[0xc4]), 0, 0, 0, false),
        (binary(2, EPOCH, 0, -1, &[0x00]), 0, 0, 0, true),
        // -1 minute and 60 seconds is offset 0, a fixed zone and not UTC.
        (binary(2, EPOCH, 0, -1, &[0x3c]), 0, 0, 0, false),
        (binary(2, EPOCH, 0, -1, &[0xc4]), 0, 0, -120, false),
        (binary(2, EPOCH, 0, -1, &[0xff]), 0, 0, -61, false),
        (
            binary(2, EPOCH, 0, i16::MIN, &[0x80]),
            0,
            0,
            -1966208,
            false,
        ),
        (binary(2, EPOCH, 0, i16::MAX, &[0x7f]), 0, 0, 1966147, false),
        (binary(1, EPOCH, 0, 0, &[]), 0, 0, 0, false),
        (binary(1, 0, 0, -1, &[]), -EPOCH, 0, 0, true),
        (binary(1, -1, 0, -1, &[]), -EPOCH - 1, 0, 0, true),
        (binary(1, EPOCH, 0, i16::MIN, &[]), 0, 0, -1966080, false),
        (binary(1, EPOCH, 0, i16::MAX, &[]), 0, 0, 1966020, false),
    ];
    for (b, unix, nsec, offset, utc) in cases {
        let t = DateTime::unmarshal_binary(&b)?;
        assert_eq!(
            (t.unix(), t.nanosecond(), t.offset(), t.zone() == Zone::Utc),
            (unix, nsec, offset, utc),
            "{}",
            hex(&b)
        );
    }
    Ok(())
}

/// Bytes that are no datetime are refused: none, a version other than 1 and 2, a length other than the version's,
/// and nanoseconds of a second or more.
#[test]
fn test_unmarshal_errors() {
    let cases = [
        vec![],
        vec![0],
        vec![3],
        vec![0xff; 15],
        binary(0, 0, 0, -1, &[]),
        vec![1],
        vec![2],
        vec![1, 2],
        binary(1, 0, 0, -1, &[0]),
        binary(2, 0, 0, -1, &[]),
        binary(1, 0, 0, -1, &[])[..14].to_vec(),
        binary(1, 0, 1_000_000_000, -1, &[]),
        binary(1, 0, 1 << 30, -1, &[]),
        binary(1, 0, u32::MAX, -1, &[]),
        binary(1, i64::MIN, 0, -1, &[]),
    ];
    for b in cases {
        let refusal = DateTime::unmarshal_binary(&b).map_err(|e| e.to_string());
        assert!(
            refusal.as_ref().is_err_and(|e| e.starts_with("datetime: ")),
            "{}: {refusal:?}",
            hex(&b)
        );
    }
}

#[test]
fn test_from_unix_carries_nanoseconds_into_seconds() -> TestResult {
    // The seconds and nanoseconds given, those kept, and the RFC 3339 text.
    let cases = [
        ((-1, 500), (-1, 500), "1969-12-31T23:59:59.0000005Z"),
        ((0, -1), (-1, 999_999_999), "1969-12-31T23:59:59.999999999Z"),
        ((0, 1_000_000_000), (1, 0), "1970-01-01T00:00:01Z"),
        (
            (5, -1_500_000_000),
            (3, 500_000_000),
            "1970-01-01T00:00:03.5Z",
        ),
        ((0, 0), (0, 0), "1970-01-01T00:00:00Z"),
        (
            (1, 999_999_999),
            (1, 999_999_999),
            "1970-01-01T00:00:01.999999999Z",
        ),
        ((-62135596800, 0), (-62135596800, 0), "0001-01-01T00:00:00Z"),
    ];
    for ((sec, nsec), (unix, ns), text) in cases {
        let t = DateTime::from_unix(sec, nsec);
        assert_eq!((t.unix(), t.nanosecond()), (unix, ns), "{sec} {nsec}");
        assert_eq!(t.zone(), Zone::Utc);
        assert_eq!(t.marshal_text()?, text);
    }
    Ok(())
}

#[test]
fn test_zero_before_and_equal() -> TestResult {
    let zero = DateTime::zero();
    assert_eq!(zero, DateTime::default());
    assert_eq!(
        (zero.unix(), zero.unix_nano()),
        (-62135596800, -6795364578871345152)
    );
    assert_eq!(
        (zero.nanosecond(), zero.zone(), zero.offset()),
        (0, Zone::Utc, 0)
    );
    assert_eq!(zero, DateTime::utc(1, 1, 1, 0, 0, 0, 0));

    let (a, b) = (DateTime::from_unix(1, 0), DateTime::from_unix(1, 1));
    assert!(a.before(&b) && !b.before(&a) && !a.before(&a));
    assert!(DateTime::from_unix(0, 999_999_999).before(&a));
    assert!(!a.equal(&b));
    // Equal compares instants; == compares the zone too.
    let east = a.with_zone(Zone::Fixed(3600));
    assert!(a.equal(&east) && !a.before(&east) && !east.before(&a));
    assert_ne!(a, east);
    assert_eq!((east.unix(), east.offset()), (1, 3600));
    assert_eq!(east.marshal_text()?, "1970-01-01T01:00:01+01:00");
    assert_eq!(east.with_zone(Zone::Utc), a);
    Ok(())
}

/// A field past its range carries, and an instant past 64 bits of seconds is the nearest one they count.
#[test]
fn test_new_with_extreme_fields_stops_at_the_ends() {
    for v in [i64::MIN, i64::MAX] {
        let got = [
            DateTime::new(CivilTime::new(2006, v, 2, 15, 4, 5, 0), Zone::Utc),
            DateTime::new(CivilTime::new(2006, 1, v, 15, 4, 5, 0), Zone::Utc),
            DateTime::new(CivilTime::new(v, 1, 2, v, v, v, v), Zone::Utc),
            DateTime::utc(2006, 1, 2, 15, v, v, v),
        ]
        .map(|t| t.unix());
        assert_eq!(got, [v; 4], "{v}");
    }
    assert_eq!(
        DateTime::from_unix(i64::MAX, i64::MAX).unix(),
        i64::MAX,
        "nanoseconds carried past the last second"
    );
    assert_eq!(
        DateTime::new(CivilTime::new(2006, 13, 2, 15, 4, 5, 0), Zone::Utc),
        DateTime::new(CivilTime::new(2007, 1, 2, 15, 4, 5, 0), Zone::Utc)
    );
}
