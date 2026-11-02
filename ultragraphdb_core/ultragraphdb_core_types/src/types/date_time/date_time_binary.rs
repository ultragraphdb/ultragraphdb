/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/time/time.go (AppendBinary, UnmarshalBinary). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::errors::types_error::TypesError;
use crate::types::date_time::{DateTime, Zone};

const VERSION_V1: u8 = 1;
const VERSION_V2: u8 = 2;
/// Go's `wallToInternal`: the seconds from year 1 to 1885, the epoch of a wall clock with a monotonic reading.
const WALL_TO_INTERNAL: i64 = (1884 * 365 + 1884 / 4 - 1884 / 100 + 1884 / 400) * 86400;

impl DateTime {
    /// Go's `MarshalBinary`: version, seconds since year 1 and nanoseconds big-endian, the zone offset in minutes
    /// (`-1` for UTC), and in version 2 the offset's leftover seconds as a signed byte.
    pub fn marshal_binary(&self) -> Result<Vec<u8>, TypesError> {
        let mut version = VERSION_V1;
        let mut offset_sec: i8 = 0;
        let offset_min: i16 = match self.zone() {
            Zone::Utc => -1,
            Zone::Fixed(offset) => {
                if offset % 60 != 0 {
                    version = VERSION_V2;
                    offset_sec = (offset % 60) as i8;
                }
                let minutes = offset / 60;
                if !(-32768..=32767).contains(&minutes) || minutes == -1 {
                    return Err(TypesError::time(
                        "Time.MarshalBinary: unexpected zone offset",
                    ));
                }
                minutes as i16
            }
        };
        let mut b = Vec::with_capacity(16);
        b.push(version);
        b.extend_from_slice(&self.internal_sec().to_be_bytes());
        b.extend_from_slice(&self.nanosecond().to_be_bytes());
        b.extend_from_slice(&offset_min.to_be_bytes());
        if version == VERSION_V2 {
            b.push(offset_sec as u8);
        }
        Ok(b)
    }

    /// Go's `UnmarshalBinary`. Version 2 adds its signed byte of leftover seconds to the offset. An offset of -60
    /// seconds is UTC; any other offset is a fixed zone.
    pub fn unmarshal_binary(data: &[u8]) -> Result<DateTime, TypesError> {
        if data.is_empty() {
            return Err(TypesError::time("Time.UnmarshalBinary: no data"));
        }
        let version = data[0];
        if version != VERSION_V1 && version != VERSION_V2 {
            return Err(TypesError::time(
                "Time.UnmarshalBinary: unsupported version",
            ));
        }
        let want = if version == VERSION_V2 { 16 } else { 15 };
        let invalid_length = || TypesError::time("Time.UnmarshalBinary: invalid length");
        if data.len() != want {
            return Err(invalid_length());
        }
        // The length is 15 or 16, checked above, so each chunk is present.
        let chunk = |start: usize| data.get(start..).ok_or_else(invalid_length);
        let sec = i64::from_be_bytes(*chunk(1)?.first_chunk().ok_or_else(invalid_length)?);
        let nsec = i32::from_be_bytes(*chunk(9)?.first_chunk().ok_or_else(invalid_length)?);
        let mut offset = i32::from(i16::from_be_bytes(
            *chunk(13)?.first_chunk().ok_or_else(invalid_length)?,
        )) * 60;
        if version == VERSION_V2 {
            // Go's int8(b): the byte read as two's complement.
            offset += i32::from(data[15].cast_signed());
        }
        let zone = if offset == -60 {
            Zone::Utc
        } else {
            Zone::Fixed(offset)
        };
        // Go sets its wall clock to the stored nanoseconds and keeps their low 30 bits as the nanosecond field. A
        // negative value sign-extends into the wall clock's monotonic bit, and Go then reads the seconds from the wall
        // clock's bits 30 to 62, after its wall epoch of 1885, instead of the stored seconds.
        let wall = i64::from(nsec) as u64;
        let sec = if nsec < 0 {
            // The shifted wall bits are 33 wide, so reading them as signed keeps their value.
            WALL_TO_INTERNAL + ((wall << 1) >> 31).cast_signed()
        } else {
            sec
        };
        Ok(DateTime::from_internal(
            sec,
            (wall & ((1 << 30) - 1)) as u32,
            zone,
        ))
    }
}
