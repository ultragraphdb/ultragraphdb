/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A datetime's binary form, the layout Go's `time.MarshalBinary` writes, which stored values hold: a version
//! byte, the seconds since January 1 of year 1 in eight bytes and the nanoseconds in four, both big-endian, and
//! the zone offset in minutes in two, `-1` for UTC. Version 2 ends in one more byte, the offset's seconds below a
//! minute.

use crate::errors::types_error::TypesError;
use crate::types::date_time::{DateTime, SECOND, YEAR_ONE_TO_UNIX, Zone};

/// The layout of an offset in whole minutes.
const VERSION_MINUTES: u8 = 1;
/// The layout with the offset's seconds below a minute in a byte of their own.
const VERSION_SECONDS: u8 = 2;
/// The offset in minutes that stands for UTC.
const UTC_MARK: i16 = -1;
/// That offset in seconds.
const UTC_MARK_SECONDS: i32 = -60;

impl DateTime {
    /// The time in its binary form: 15 bytes, or 16 for a zone offset that is no whole minute.
    ///
    /// # Errors
    ///
    /// [`TypesError::time`] for a fixed offset the form cannot hold: one of minus one minute, which reads back
    /// as UTC, one past 16 bits of minutes, and an instant whose seconds since year 1 pass 64 bits.
    pub fn marshal_binary(&self) -> Result<Vec<u8>, TypesError> {
        let unwritable = |offset: i32| {
            TypesError::time(format!(
                "datetime: a zone offset of {offset} seconds has no binary form"
            ))
        };
        let (minutes, seconds) = match self.zone() {
            Zone::Utc => (UTC_MARK, 0_i8),
            Zone::Fixed(offset) => {
                let minutes = i16::try_from(offset / 60).map_err(|_| unwritable(offset))?;
                if minutes == UTC_MARK {
                    return Err(unwritable(offset));
                }
                // A remainder of a division by 60 lies within a signed byte.
                (minutes, i8::try_from(offset % 60).unwrap_or(0))
            }
        };
        let since_year_one = self.unix().checked_add(YEAR_ONE_TO_UNIX).ok_or_else(|| {
            TypesError::time("datetime: an instant past the binary form's seconds")
        })?;
        let mut bytes = Vec::with_capacity(16);
        bytes.push(if seconds == 0 {
            VERSION_MINUTES
        } else {
            VERSION_SECONDS
        });
        bytes.extend_from_slice(&since_year_one.to_be_bytes());
        bytes.extend_from_slice(&self.nanosecond().to_be_bytes());
        bytes.extend_from_slice(&minutes.to_be_bytes());
        if seconds != 0 {
            bytes.extend_from_slice(&seconds.to_be_bytes());
        }
        Ok(bytes)
    }

    /// The time `data` holds in its binary form.
    ///
    /// # Errors
    ///
    /// [`TypesError::time`] for no bytes, a version other than 1 and 2, a length other than the version's 15 or
    /// 16, nanoseconds of a second or more, and seconds before what 64 bits count from 1970.
    pub fn unmarshal_binary(data: &[u8]) -> Result<DateTime, TypesError> {
        let Some((&version, rest)) = data.split_first() else {
            return Err(TypesError::time("datetime: no bytes"));
        };
        let want = match version {
            VERSION_MINUTES => 15,
            VERSION_SECONDS => 16,
            other => {
                return Err(TypesError::time(format!(
                    "datetime: a binary form of version {other}, where 1 and 2 are read"
                )));
            }
        };
        let wrong_length = || {
            TypesError::time(format!(
                "datetime: {} bytes, where version {version} takes {want}",
                data.len()
            ))
        };
        if data.len() != want {
            return Err(wrong_length());
        }
        let (since_year_one, rest) = rest.split_first_chunk().ok_or_else(wrong_length)?;
        let (nanos, rest) = rest.split_first_chunk().ok_or_else(wrong_length)?;
        let (minutes, rest) = rest.split_first_chunk().ok_or_else(wrong_length)?;
        let nanos = u32::from_be_bytes(*nanos);
        if i64::from(nanos) >= SECOND {
            return Err(TypesError::time(format!(
                "datetime: {nanos} nanoseconds, where a second holds fewer"
            )));
        }
        let unix = i64::from_be_bytes(*since_year_one)
            .checked_sub(YEAR_ONE_TO_UNIX)
            .ok_or_else(|| TypesError::time("datetime: seconds before what the instant counts"))?;
        let minutes = i16::from_be_bytes(*minutes);
        // Sixteen bits of minutes and a byte of seconds lie far inside 32 bits.
        let seconds = rest.first().map_or(0, |byte| i32::from(byte.cast_signed()));
        // UTC is the offset of minus one minute, however its minutes and seconds make it up.
        let zone = match i32::from(minutes) * 60 + seconds {
            UTC_MARK_SECONDS => Zone::Utc,
            offset => Zone::Fixed(offset),
        };
        Ok(DateTime::from_unix_zone(unix, nanos, zone))
    }
}
