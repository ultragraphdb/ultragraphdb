/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26, src/time/time.go. Changed for ultragraphdb: translated to Rust; the part of time.Time that
 * Dgraph's datetime values use (instant, fixed zone offset, binary and RFC 3339 forms, the layouts ParseTime tries).
 * Copyright (c) 2026. Marvin Hansen.
 */

mod date_time_binary;
mod date_time_calendar;
mod date_time_format;
pub(crate) mod date_time_parse;

use crate::types::date_time::date_time_calendar::{
    ABSOLUTE_TO_UNIX, INTERNAL_TO_UNIX, UNIX_TO_INTERNAL, date_to_abs_days,
};

/// The zone a [`DateTime`] is presented in: UTC, or a fixed offset east of UTC in seconds.
///
/// Go also has `time.Local`. Dgraph never sets it, and a time in the local zone writes the same binary and text
/// forms as one in a fixed zone of the same offset, so this port records every parsed offset as `Fixed`, even 0:
/// only `Z` (and the zone name `UTC`) gives `Utc`, as in a Dgraph whose local zone is not Go's `time.UTC`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Zone {
    /// Go's `time.UTC`; binary offset `-1` (`FF FF`).
    Utc,
    /// A fixed offset east of UTC, in seconds.
    Fixed(i32),
}

/// Go's `time.Time` without the monotonic clock: an instant (seconds since January 1, year 1, and nanoseconds)
/// and the zone it is presented in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DateTime {
    sec: i64,
    nsec: u32,
    zone: Zone,
}

impl Default for DateTime {
    fn default() -> Self {
        Self::zero()
    }
}

impl DateTime {
    /// Go's zero `time.Time{}`: January 1, year 1, 00:00:00 UTC.
    pub const fn zero() -> Self {
        Self {
            sec: 0,
            nsec: 0,
            zone: Zone::Utc,
        }
    }

    /// Go's `time.Date(year, month, day, hour, min, sec, nsec, loc)`, normalising out-of-range fields as Go does.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        year: i64,
        month: i64,
        day: i64,
        hour: i64,
        min: i64,
        sec: i64,
        nsec: i64,
        zone: Zone,
    ) -> Self {
        let (year, m) = norm(year, month - 1, 12);
        let month = m + 1;
        let (sec, nsec) = norm(sec, nsec, 1_000_000_000);
        let (min, sec) = norm(min, sec, 60);
        let (hour, min) = norm(hour, min, 60);
        let (day, hour) = norm(day, hour, 24);
        let mut unix = (date_to_abs_days(year, month as u32, day) as i64)
            .wrapping_mul(86400)
            .wrapping_add(hour * 3600 + min * 60 + sec)
            .wrapping_add(ABSOLUTE_TO_UNIX);
        if let Zone::Fixed(offset) = zone {
            unix = unix.wrapping_sub(i64::from(offset));
        }
        Self::from_unix_zone(unix, nsec as u32, zone)
    }

    /// `time.Date(...)` in UTC.
    pub fn utc(year: i64, month: i64, day: i64, hour: i64, min: i64, sec: i64, nsec: i64) -> Self {
        Self::new(year, month, day, hour, min, sec, nsec, Zone::Utc)
    }

    /// Go's `time.Unix(sec, nsec).UTC()`.
    pub fn from_unix(sec: i64, nsec: i64) -> Self {
        let (mut sec, mut nsec) = (sec, nsec);
        if !(0..1_000_000_000).contains(&nsec) {
            let n = nsec / 1_000_000_000;
            sec = sec.wrapping_add(n);
            nsec -= n * 1_000_000_000;
            if nsec < 0 {
                nsec += 1_000_000_000;
                sec = sec.wrapping_sub(1);
            }
        }
        Self::from_unix_zone(sec, nsec as u32, Zone::Utc)
    }

    pub(crate) fn from_unix_zone(unix: i64, nsec: u32, zone: Zone) -> Self {
        Self {
            sec: unix.wrapping_add(UNIX_TO_INTERNAL),
            nsec,
            zone,
        }
    }

    pub(crate) fn from_internal(sec: i64, nsec: u32, zone: Zone) -> Self {
        Self { sec, nsec, zone }
    }

    /// The same instant presented in another zone (Go's `In`, `UTC`).
    pub fn with_zone(self, zone: Zone) -> Self {
        Self { zone, ..self }
    }

    /// The zone this time is presented in.
    pub fn zone(&self) -> Zone {
        self.zone
    }

    /// The zone's offset east of UTC in seconds.
    pub fn offset(&self) -> i32 {
        match self.zone {
            Zone::Utc => 0,
            Zone::Fixed(o) => o,
        }
    }

    /// Go's `Unix()`: seconds since 1970-01-01 UTC.
    pub fn unix(&self) -> i64 {
        self.sec.wrapping_add(INTERNAL_TO_UNIX)
    }

    /// Go's `UnixNano()`, wrapping outside the years 1678 to 2262 as Go's does.
    pub fn unix_nano(&self) -> i64 {
        self.unix()
            .wrapping_mul(1_000_000_000)
            .wrapping_add(i64::from(self.nsec))
    }

    /// Go's `Nanosecond()`.
    pub fn nanosecond(&self) -> u32 {
        self.nsec
    }

    pub(crate) fn internal_sec(&self) -> i64 {
        self.sec
    }

    /// Go's `Before`: whether this instant is earlier than `other`.
    pub fn before(&self, other: &DateTime) -> bool {
        self.sec < other.sec || self.sec == other.sec && self.nsec < other.nsec
    }

    /// Go's `Equal`: the same instant, whatever the zones.
    pub fn equal(&self, other: &DateTime) -> bool {
        self.sec == other.sec && self.nsec == other.nsec
    }
}

/// Go's `norm`: moves `lo` into `[0, base)`, carrying into `hi`.
fn norm(hi: i64, lo: i64, base: i64) -> (i64, i64) {
    let (mut hi, mut lo) = (hi, lo);
    if lo < 0 {
        let n = (-lo - 1) / base + 1;
        hi -= n;
        lo += n * base;
    }
    if lo >= base {
        let n = lo / base;
        hi += n;
        lo -= n * base;
    }
    (hi, lo)
}
