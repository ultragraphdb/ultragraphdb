/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A datetime read from text, in the shapes a datetime value is written in: a year, `2018`; a year and month,
//! `2018-10`; a date, `2018-10-03`; a date and time with `T` between them, with or without a zone,
//! `2018-10-03T08:05:01Z`, `2018-10-03T08:05:01.5+05:30`; and a date and time with a numeric offset and a zone
//! abbreviation, `2018-10-03 08:05:01 -0700 MST`.

use crate::errors::types_error::TypesError;
use crate::types::civil_time::CivilTime;
use crate::types::date_time::date_time_calendar::days_in;
use crate::types::date_time::date_time_format::MAX_OFFSET;
use crate::types::date_time::{DateTime, Zone};

/// What is left of a text being read.
struct Scan<'a> {
    rest: &'a [u8],
}

impl Scan<'_> {
    /// The number `count` ASCII digits spell, taken off the front; `None` unless that many stand there.
    fn digits(&mut self, count: usize) -> Option<i64> {
        let (digits, rest) = self.rest.split_at_checked(count)?;
        let number = digits.iter().try_fold(0_i64, |number, digit| {
            digit
                .is_ascii_digit()
                .then(|| number * 10 + i64::from(digit - b'0'))
        })?;
        self.rest = rest;
        Some(number)
    }

    /// Takes `byte` off the front when it stands there.
    fn eat(&mut self, byte: u8) -> bool {
        match self.rest.split_first() {
            Some((&first, rest)) if first == byte => {
                self.rest = rest;
                true
            }
            _ => false,
        }
    }

    fn is_done(&self) -> bool {
        self.rest.is_empty()
    }
}

/// The datetime `text` holds.
///
/// A year has four digits and every other field two. A time's seconds may carry a fraction after `.` or `,`, read
/// to the nanosecond with further digits dropped. A text without a zone is UTC; `Z` is UTC; a numeric offset is a
/// fixed zone of at most 23:59. In the form with an abbreviation the numeric offset decides the zone, and the
/// abbreviation is three to five capital letters; an offset of zero named `UTC` is UTC.
///
/// # Errors
///
/// [`TypesError::time`] naming the text and what it breaks: a shape that is none of the above, or a month, day,
/// hour, minute, second or zone offset out of range.
pub(crate) fn parse(text: &[u8]) -> Result<DateTime, TypesError> {
    let refuse = |why: &str| {
        TypesError::time(format!(
            "datetime {:?}: {why}",
            String::from_utf8_lossy(text)
        ))
    };
    let shape = || refuse("not a year, a year and month, a date, or a date and time");
    let mut scan = Scan { rest: text };
    let year = scan.digits(4).ok_or_else(shape)?;
    let (mut month, mut day) = (1, 1);
    let mut time = (0, 0, 0, 0);
    let mut zone = Zone::Utc;
    if scan.eat(b'-') {
        month = scan.digits(2).ok_or_else(shape)?;
        if scan.eat(b'-') {
            day = scan.digits(2).ok_or_else(shape)?;
            if scan.eat(b'T') {
                time = clock(&mut scan).ok_or_else(shape)?;
                zone = zone_after_t(&mut scan).ok_or_else(shape)?;
            } else if scan.eat(b' ') {
                time = clock(&mut scan).ok_or_else(shape)?;
                zone = zone_with_name(&mut scan).ok_or_else(shape)?;
            }
        }
    }
    if !scan.is_done() {
        return Err(shape());
    }
    let (hour, min, sec, nsec) = time;
    if !(1..=12).contains(&month) {
        return Err(refuse("month out of range"));
    }
    if !(1..=days_in(month, year)).contains(&day) {
        return Err(refuse("day out of range"));
    }
    for (field, bound, name) in [(hour, 24, "hour"), (min, 60, "minute"), (sec, 60, "second")] {
        if field >= bound {
            return Err(refuse(&format!("{name} out of range")));
        }
    }
    if let Zone::Fixed(offset) = zone
        && offset.unsigned_abs() > MAX_OFFSET.unsigned_abs()
    {
        return Err(refuse("zone offset out of range"));
    }
    Ok(DateTime::new(
        CivilTime::new(year, month, day, hour, min, sec, nsec),
        zone,
    ))
}

/// `hh:mm:ss` and its fraction: the hour, minute, second and nanoseconds.
fn clock(scan: &mut Scan<'_>) -> Option<(i64, i64, i64, i64)> {
    let hour = scan.digits(2)?;
    let min = scan.eat(b':').then(|| scan.digits(2))??;
    let sec = scan.eat(b':').then(|| scan.digits(2))??;
    let nsec = if scan.eat(b'.') || scan.eat(b',') {
        fraction(scan)?
    } else {
        0
    };
    Some((hour, min, sec, nsec))
}

/// The nanoseconds a run of one digit or more spells as a fraction of a second, digits past the ninth dropped.
fn fraction(scan: &mut Scan<'_>) -> Option<i64> {
    let count = scan.rest.iter().take_while(|b| b.is_ascii_digit()).count();
    if count == 0 {
        return None;
    }
    let kept = count.min(9);
    let nanos = (kept..9).fold(scan.digits(kept)?, |nanos, _| nanos * 10);
    let (_dropped, rest) = scan.rest.split_at_checked(count - kept)?;
    scan.rest = rest;
    Some(nanos)
}

/// The zone after a time that follows `T`: none for UTC, `Z`, or `+hh:mm`.
fn zone_after_t(scan: &mut Scan<'_>) -> Option<Zone> {
    if scan.is_done() || scan.eat(b'Z') {
        return Some(Zone::Utc);
    }
    let sign = sign(scan)?;
    let hours = scan.digits(2)?;
    let minutes = scan.eat(b':').then(|| scan.digits(2))??;
    offset(sign, hours, minutes)
}

/// The zone after a time that follows a space: ` +hhmm`, a space and an abbreviation of three to five capitals.
fn zone_with_name(scan: &mut Scan<'_>) -> Option<Zone> {
    if !scan.eat(b' ') {
        return None;
    }
    let sign = sign(scan)?;
    let (hours, minutes) = (scan.digits(2)?, scan.digits(2)?);
    if !scan.eat(b' ') {
        return None;
    }
    let name = scan.rest;
    if !(3..=5).contains(&name.len()) || !name.iter().all(u8::is_ascii_uppercase) {
        return None;
    }
    scan.rest = &[];
    match offset(sign, hours, minutes)? {
        Zone::Fixed(0) if name == b"UTC" => Some(Zone::Utc),
        zone @ (Zone::Utc | Zone::Fixed(_)) => Some(zone),
    }
}

/// An offset's sign: 1 for `+`, -1 for `-`.
fn sign(scan: &mut Scan<'_>) -> Option<i64> {
    if scan.eat(b'+') {
        Some(1)
    } else if scan.eat(b'-') {
        Some(-1)
    } else {
        None
    }
}

/// The fixed zone of an offset of `hours` and `minutes`; `None` for minutes of 60 or more. Two digits each keep
/// the seconds far inside 32 bits.
fn offset(sign: i64, hours: i64, minutes: i64) -> Option<Zone> {
    if minutes >= 60 {
        return None;
    }
    i32::try_from(sign * (hours * 3_600 + minutes * 60))
        .ok()
        .map(Zone::Fixed)
}
