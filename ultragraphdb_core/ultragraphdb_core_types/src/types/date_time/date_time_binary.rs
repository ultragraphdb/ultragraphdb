/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26, src/time/time.go (AppendBinary, UnmarshalBinary). Changed for ultragraphdb: translated to Rust.
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
    /// (`-1` for UTC), and in version 2 the offset's leftover seconds.
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

    /// Go's `UnmarshalBinary`. An offset of -1 minute is UTC; any other offset is a fixed zone.
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
        if data.len() != want {
            return Err(TypesError::time("Time.UnmarshalBinary: invalid length"));
        }
        // The length is 15 or 16, checked above, so each range holds exactly the bytes it is read as.
        let sec = i64::from_be_bytes(data[1..9].try_into().expect("bytes 1 to 9 of 15 or more"));
        let nsec = i32::from_be_bytes(data[9..13].try_into().expect("bytes 9 to 13 of 15 or more"));
        let mut offset = i32::from(i16::from_be_bytes(
            data[13..15]
                .try_into()
                .expect("bytes 13 to 15 of 15 or more"),
        )) * 60;
        if version == VERSION_V2 {
            offset += i32::from(data[15]);
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
            WALL_TO_INTERNAL + ((wall << 1) >> 31) as i64
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
