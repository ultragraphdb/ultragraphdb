/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod date_time_binary;
mod date_time_calendar;
mod date_time_format;
pub(crate) mod date_time_parse;

use crate::types::civil_time::CivilTime;
use crate::types::date_time::date_time_calendar::days_from_civil;

/// The seconds of a day.
pub(crate) const DAY: i64 = 86_400;
/// The nanoseconds of a second.
pub(crate) const SECOND: i64 = 1_000_000_000;
/// The seconds from January 1 of year 1 to January 1 of 1970, both at 00:00:00 UTC.
pub(crate) const YEAR_ONE_TO_UNIX: i64 = 62_135_596_800;

/// The zone a [`DateTime`] is presented in: UTC, or a fixed offset east of UTC in seconds.
///
/// A text ending in `Z` reads as `Utc` and one ending in a numeric offset as `Fixed`, an offset of zero included:
/// the two write the same text and differ in their binary form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Zone {
    /// UTC; binary offset `-1` (`FF FF`).
    Utc,
    /// A fixed offset east of UTC, in seconds.
    Fixed(i32),
}

/// An instant, as seconds since 1970-01-01T00:00:00 UTC and nanoseconds within the second, and the zone it is
/// presented in. Two times are the same instant whatever their zones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DateTime {
    unix: i64,
    nsec: u32,
    zone: Zone,
}

impl Default for DateTime {
    fn default() -> Self {
        Self::zero()
    }
}

impl DateTime {
    /// The zero time: January 1, year 1, 00:00:00 UTC.
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            unix: -YEAR_ONE_TO_UNIX,
            nsec: 0,
            zone: Zone::Utc,
        }
    }

    /// The instant `civil` names when read in `zone`. A field past its range carries into the next: month 13 is
    /// January of the year after, day 0 the last day of the month before. An instant past what the seconds since
    /// 1970 count in 64 bits is the nearest one they do.
    #[must_use]
    pub fn new(civil: CivilTime, zone: Zone) -> Self {
        let wide = i128::from;
        // Months from zero, so the carry into the year is a division.
        let months = wide(civil.month) - 1;
        let year = wide(civil.year) + months.div_euclid(12);
        let month = months.rem_euclid(12) + 1;
        let days = days_from_civil(year, month, 1) + wide(civil.day) - 1;
        let nanos = wide(civil.nsec);
        let offset = match zone {
            Zone::Utc => 0,
            Zone::Fixed(offset) => i128::from(offset),
        };
        let seconds = days * wide(DAY)
            + wide(civil.hour) * 3_600
            + wide(civil.min) * 60
            + wide(civil.sec)
            + nanos.div_euclid(wide(SECOND))
            - offset;
        let unix = i64::try_from(seconds).unwrap_or(if seconds < 0 { i64::MIN } else { i64::MAX });
        // The remainder lies from 0 to 999,999,999.
        let nsec = u32::try_from(nanos.rem_euclid(wide(SECOND))).unwrap_or(0);
        Self { unix, nsec, zone }
    }

    /// The instant of the given civil time in UTC, as [`DateTime::new`] reads it.
    #[must_use]
    pub fn utc(year: i64, month: i64, day: i64, hour: i64, min: i64, sec: i64, nsec: i64) -> Self {
        Self::new(
            CivilTime::new(year, month, day, hour, min, sec, nsec),
            Zone::Utc,
        )
    }

    /// The instant `sec` seconds and `nsec` nanoseconds after 1970-01-01T00:00:00 UTC, in UTC. Nanoseconds past a
    /// second carry into the seconds, which stop at the ends of 64 bits.
    #[must_use]
    pub fn from_unix(sec: i64, nsec: i64) -> Self {
        Self::from_unix_zone(
            sec.saturating_add(nsec.div_euclid(SECOND)),
            // The remainder lies from 0 to 999,999,999.
            u32::try_from(nsec.rem_euclid(SECOND)).unwrap_or(0),
            Zone::Utc,
        )
    }

    /// The instant `unix` seconds and `nsec` nanoseconds, below a second, after 1970, presented in `zone`.
    pub(crate) fn from_unix_zone(unix: i64, nsec: u32, zone: Zone) -> Self {
        Self { unix, nsec, zone }
    }

    /// The same instant presented in another zone.
    #[must_use]
    pub fn with_zone(self, zone: Zone) -> Self {
        Self { zone, ..self }
    }

    /// The zone this time is presented in.
    #[must_use]
    pub fn zone(&self) -> Zone {
        self.zone
    }

    /// The zone's offset east of UTC in seconds.
    #[must_use]
    pub fn offset(&self) -> i32 {
        match self.zone {
            Zone::Utc => 0,
            Zone::Fixed(o) => o,
        }
    }

    /// Seconds since 1970-01-01T00:00:00 UTC.
    #[must_use]
    pub fn unix(&self) -> i64 {
        self.unix
    }

    /// Nanoseconds since 1970-01-01T00:00:00 UTC, wrapping outside the years 1678 to 2262, which 64 bits hold.
    #[must_use]
    pub fn unix_nano(&self) -> i64 {
        self.unix
            .wrapping_mul(SECOND)
            .wrapping_add(i64::from(self.nsec))
    }

    /// The nanoseconds within the second.
    #[must_use]
    pub fn nanosecond(&self) -> u32 {
        self.nsec
    }

    /// Whether this instant is earlier than `other`.
    #[must_use]
    pub fn before(&self, other: &DateTime) -> bool {
        (self.unix, self.nsec) < (other.unix, other.nsec)
    }

    /// Whether this is the same instant as `other`, whatever the zones.
    #[must_use]
    pub fn equal(&self, other: &DateTime) -> bool {
        (self.unix, self.nsec) == (other.unix, other.nsec)
    }
}
