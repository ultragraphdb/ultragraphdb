/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_types::{DateTime, Zone};

#[test]
fn test_utc_binary_is_version_1() {
    let b = DateTime::utc(2018, 10, 3, 8, 5, 1, 0)
        .marshal_binary()
        .unwrap();
    assert_eq!(b.len(), 15);
    assert_eq!(b[0], 1);
    assert_eq!(&b[13..], &[0xff, 0xff]);
}

#[test]
fn test_seconds_offset_is_version_2() {
    let t = DateTime::new(
        2018,
        5,
        28,
        14,
        41,
        57,
        0,
        Zone::Fixed(5 * 3600 + 30 * 60 + 15),
    );
    let b = t.marshal_binary().unwrap();
    assert_eq!((b.len(), b[0], b[15]), (16, 2, 15));
    assert_eq!(DateTime::unmarshal_binary(&b).unwrap(), t);
}

#[test]
fn test_minus_one_minute_offset_is_refused() {
    let t = DateTime::new(2018, 5, 28, 14, 41, 57, 0, Zone::Fixed(-60));
    assert_eq!(
        t.marshal_binary().unwrap_err().to_string(),
        "Time.MarshalBinary: unexpected zone offset"
    );
}

#[test]
fn test_unmarshal_errors() {
    assert_eq!(
        DateTime::unmarshal_binary(&[]).unwrap_err().to_string(),
        "Time.UnmarshalBinary: no data"
    );
    assert_eq!(
        DateTime::unmarshal_binary(&[3]).unwrap_err().to_string(),
        "Time.UnmarshalBinary: unsupported version"
    );
    assert_eq!(
        DateTime::unmarshal_binary(&[1, 2]).unwrap_err().to_string(),
        "Time.UnmarshalBinary: invalid length"
    );
}

#[test]
fn test_marshal_text() {
    let t = DateTime::new(2006, 1, 2, 15, 4, 5, 120_000_000, Zone::Fixed(-7 * 3600));
    assert_eq!(t.marshal_text().unwrap(), "2006-01-02T15:04:05.12-07:00");
    assert_eq!(
        DateTime::utc(10000, 1, 1, 0, 0, 0, 0)
            .marshal_text()
            .unwrap_err()
            .to_string(),
        "Time.MarshalText: year outside of range [0,9999]"
    );
    assert_eq!(
        DateTime::zero().marshal_text().unwrap(),
        "0001-01-01T00:00:00Z"
    );
}

#[test]
fn test_unix() {
    let t = DateTime::from_unix(-1, 500);
    assert_eq!((t.unix(), t.nanosecond()), (-1, 500));
    assert_eq!(DateTime::from_unix(0, -1).unix(), -1);
    assert!(DateTime::from_unix(1, 0).before(&DateTime::from_unix(1, 1)));
    assert!(
        DateTime::from_unix(0, 0).equal(&DateTime::from_unix(0, 0).with_zone(Zone::Fixed(3600)))
    );
    assert_eq!(DateTime::zero().unix(), -62135596800);
}
