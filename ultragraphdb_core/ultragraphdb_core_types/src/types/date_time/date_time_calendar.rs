/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The proleptic Gregorian calendar, by counting days in eras of 400 years, each of 146,097 days, with the year
//! taken to begin in March so that the leap day is its last. Every count is in 128 bits, which no year or day
//! count of 64 bits overflows. The method is the one H. Hinnant describes in "chrono-Compatible Low-Level Date
//! Algorithms" (2013).

use crate::types::civil_time::CivilTime;
use crate::types::date_time::{DAY, DateTime};

/// The days of an era of 400 years.
const ERA_DAYS: i128 = 146_097;
/// The days from March 1 of year 0 to January 1 of 1970.
const MARCH_YEAR_ZERO_TO_UNIX: i128 = 719_468;

/// The days from 1970-01-01 to the given day, negative before it. `month` is 1 to 12; `day` may lie past the
/// month, and counts on from its first.
pub(crate) fn days_from_civil(year: i128, month: i128, day: i128) -> i128 {
    // January and February belong to the year before, as its months 11 and 12 from March.
    let year = if month <= 2 { year - 1 } else { year };
    let month_from_march = (month + 9) % 12;
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let day_of_year = (153 * month_from_march + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * ERA_DAYS + day_of_era - MARCH_YEAR_ZERO_TO_UNIX
}

/// The year, month and day of the day `days` after 1970-01-01.
fn civil_from_days(days: i128) -> (i128, i128, i128) {
    let days = days + MARCH_YEAR_ZERO_TO_UNIX;
    let era = days.div_euclid(ERA_DAYS);
    let day_of_era = days.rem_euclid(ERA_DAYS);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    let year = year_of_era + era * 400 + i128::from(month <= 2);
    (year, month, day)
}

/// Whether `year` is a leap year.
pub(crate) fn is_leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// The days of `month`, 1 to 12, in `year`; none for another month.
pub(crate) fn days_in(month: i64, year: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

impl DateTime {
    /// The civil time this instant reads as in its zone, every field in range.
    pub(crate) fn civil(&self) -> CivilTime {
        let local = i128::from(self.unix()) + i128::from(self.offset());
        let (year, month, day) = civil_from_days(local.div_euclid(i128::from(DAY)));
        let second_of_day = local.rem_euclid(i128::from(DAY));
        // A year of an instant 64 bits of seconds hold lies far inside 64 bits, and the rest are small.
        let narrow = |v: i128| i64::try_from(v).unwrap_or(0);
        CivilTime::new(
            narrow(year),
            narrow(month),
            narrow(day),
            narrow(second_of_day / 3_600),
            narrow(second_of_day / 60 % 60),
            narrow(second_of_day % 60),
            i64::from(self.nanosecond()),
        )
    }
}
