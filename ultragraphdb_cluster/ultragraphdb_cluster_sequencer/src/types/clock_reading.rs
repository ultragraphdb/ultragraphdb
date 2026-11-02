/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// One reading of the clock the sequencer is given: a monotonic time, which spaces closes and never steps back, and
/// the wall-clock time a close records, which may. Both in microseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClockReading {
    monotonic_us: u64,
    wall_us: u64,
}

impl ClockReading {
    /// The reading at monotonic time `monotonic_us` and wall-clock time `wall_us`.
    #[must_use]
    pub fn new(monotonic_us: u64, wall_us: u64) -> Self {
        Self {
            monotonic_us,
            wall_us,
        }
    }

    /// The monotonic time in microseconds.
    #[must_use]
    pub fn monotonic_us(self) -> u64 {
        self.monotonic_us
    }

    /// The wall-clock time in microseconds.
    #[must_use]
    pub fn wall_us(self) -> u64 {
        self.wall_us
    }
}
