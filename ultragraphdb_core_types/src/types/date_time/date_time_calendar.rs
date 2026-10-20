/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26, src/time/time.go (absolute dates). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::date_time::{DateTime, Zone};

const SECONDS_PER_DAY: u64 = 86400;
const MARCH_THRU_DECEMBER: u64 = 31 + 30 + 31 + 30 + 31 + 31 + 30 + 31 + 30 + 31;
const ABSOLUTE_YEARS: u64 = 292277022400;
const ABSOLUTE_TO_INTERNAL: i64 =
    -(((ABSOLUTE_YEARS / 400 * 146097) + MARCH_THRU_DECEMBER) as i64 * SECONDS_PER_DAY as i64);
const INTERNAL_TO_ABSOLUTE: i64 = -ABSOLUTE_TO_INTERNAL;
pub(crate) const UNIX_TO_INTERNAL: i64 =
    (1969 * 365 + 1969 / 4 - 1969 / 100 + 1969 / 400) * SECONDS_PER_DAY as i64;
pub(crate) const INTERNAL_TO_UNIX: i64 = -UNIX_TO_INTERNAL;
pub(crate) const ABSOLUTE_TO_UNIX: i64 = ABSOLUTE_TO_INTERNAL + INTERNAL_TO_UNIX;
const UNIX_TO_ABSOLUTE: i64 = UNIX_TO_INTERNAL + INTERNAL_TO_ABSOLUTE;

/// Go's `dateToAbsDays`: days from the absolute epoch to a date; month and day may be out of range.
pub(crate) fn date_to_abs_days(year: i64, month: u32, day: i64) -> u64 {
    let mut amonth = month;
    let jan_feb: u32 = u32::from(amonth < 3);
    amonth = amonth.wrapping_add(12 * jan_feb);
    let y = (year as u64)
        .wrapping_sub(u64::from(jan_feb))
        .wrapping_add(ABSOLUTE_YEARS);
    let ayday = 979u32.wrapping_mul(amonth).wrapping_sub(2919) >> 5;
    let century = y / 100;
    let cyear = (y % 100) as u32;
    let cday = 1461 * cyear / 4;
    let centurydays = 146097u64.wrapping_mul(century) / 4;
    centurydays.wrapping_add((i64::from(cday.wrapping_add(ayday)) + day - 1) as u64)
}

/// Year, month, day, hour, minute, second of a time as presented in its zone.
pub(crate) struct Civil {
    pub(crate) year: i64,
    pub(crate) month: i64,
    pub(crate) day: i64,
    pub(crate) hour: i64,
    pub(crate) min: i64,
    pub(crate) sec: i64,
}

impl DateTime {
    fn abs(&self) -> u64 {
        let offset = match self.zone() {
            Zone::Utc => 0,
            Zone::Fixed(o) => i64::from(o),
        };
        self.unix()
            .wrapping_add(offset)
            .wrapping_add(UNIX_TO_ABSOLUTE) as u64
    }

    /// Go's `Date()` and `Clock()`.
    pub(crate) fn civil(&self) -> Civil {
        let abs = self.abs();
        let days = abs / SECONDS_PER_DAY;
        let d = 4u64.wrapping_mul(days).wrapping_add(3);
        let century = d / 146097;
        let cd = ((d % 146097) as u32) | 3;
        let prod = u64::from(2939745u32) * u64::from(cd);
        let cyear = (prod >> 32) as u32;
        let ayday = (prod as u32) / 2939745 / 4;
        let md = 2141 * ayday + 197913;
        let amonth = i64::from(md >> 16);
        let day = 1 + i64::from((md & 0xffff) / 2141);
        let jan_feb = i64::from(u64::from(ayday) >= MARCH_THRU_DECEMBER);
        let year = (century.wrapping_mul(100).wrapping_sub(ABSOLUTE_YEARS) as i64)
            + i64::from(cyear)
            + jan_feb;
        let month = amonth - jan_feb * 12;
        let s = (abs % SECONDS_PER_DAY) as i64;
        Civil {
            year,
            month,
            day,
            hour: s / 3600,
            min: s % 3600 / 60,
            sec: s % 60,
        }
    }
}

/// Go's `isLeap`.
pub(crate) fn is_leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// Go's `daysIn`.
pub(crate) fn days_in(month: i64, year: i64) -> i64 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}
