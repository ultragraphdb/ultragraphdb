/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::types_error::TypesError;
use crate::types::date_time::DateTime;

/// The widest zone offset RFC 3339 writes: 23 hours and 59 minutes, in seconds.
pub(crate) const MAX_OFFSET: i32 = 23 * 3_600 + 59 * 60;

impl DateTime {
    /// The time as RFC 3339 text in its zone: the date, `T`, the clock time, the nanoseconds when there are any
    /// with their trailing zeros dropped, and `Z` for an offset of zero or the offset as `+hh:mm`. An offset's
    /// seconds have no place in the text and are left out.
    ///
    /// # Errors
    ///
    /// [`TypesError::time`] for a year outside 0 to 9999 and for an offset past 23:59, neither of which RFC 3339
    /// writes.
    pub fn marshal_text(&self) -> Result<String, TypesError> {
        let c = self.civil();
        if !(0..=9999).contains(&c.year) {
            return Err(TypesError::time(format!(
                "datetime: a year of {} is outside 0 to 9999",
                c.year
            )));
        }
        let offset = self.offset();
        if offset.unsigned_abs() > MAX_OFFSET.unsigned_abs() {
            return Err(TypesError::time(format!(
                "datetime: a zone offset of {offset} seconds is past 23:59"
            )));
        }
        let mut text = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            c.year, c.month, c.day, c.hour, c.min, c.sec
        );
        if self.nanosecond() != 0 {
            let fraction = format!(".{:09}", self.nanosecond());
            text.push_str(fraction.trim_end_matches('0'));
        }
        if offset == 0 {
            text.push('Z');
        } else {
            // The sign is the whole minutes': an offset below a minute writes as `+00:00`.
            let minutes = offset.unsigned_abs() / 60;
            let sign = if offset <= -60 { '-' } else { '+' };
            text.push_str(&format!("{sign}{:02}:{:02}", minutes / 60, minutes % 60));
        }
        Ok(text)
    }
}
