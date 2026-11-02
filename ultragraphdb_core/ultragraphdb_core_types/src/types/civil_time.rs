/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A civil date and clock time, the fields of Go's `time.Date`: year, month, day, hour, minute, second and
/// nanosecond as read in some zone. A field may be out of range; [`crate::DateTime::new`] normalises it as Go does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CivilTime {
    pub(crate) year: i64,
    pub(crate) month: i64,
    pub(crate) day: i64,
    pub(crate) hour: i64,
    pub(crate) min: i64,
    pub(crate) sec: i64,
    pub(crate) nsec: i64,
}

impl CivilTime {
    /// The civil time of the given year, month, day, hour, minute, second and nanosecond.
    pub const fn new(
        year: i64,
        month: i64,
        day: i64,
        hour: i64,
        min: i64,
        sec: i64,
        nsec: i64,
    ) -> Self {
        Self {
            year,
            month,
            day,
            hour,
            min,
            sec,
            nsec,
        }
    }
}
