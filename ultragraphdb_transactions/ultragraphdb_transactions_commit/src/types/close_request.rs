/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::setting_error::SettingError;

/// `close_request_ms`: how long a partition waits for the close it asked the sequencer for, its open epoch full, before
/// it counts the sequencer as unreachable and refuses its held and new commits as busy until the close arrives
/// (commit-rounds, "Commit versions are built from the epoch"). From 1 to 60,000 ms, and at least twice the sequencer's
/// `epoch_ms_max`, so a reachable sequencer always answers within it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CloseRequest {
    ms: u64,
}

impl Default for CloseRequest {
    /// The longest wait, 60,000 ms.
    fn default() -> Self {
        Self { ms: Self::MS_MAX }
    }
}

impl CloseRequest {
    /// The fewest milliseconds.
    pub const MS_MIN: u64 = 1;
    /// The most milliseconds.
    pub const MS_MAX: u64 = 60_000;

    /// `close_request_ms` of `ms` beside the sequencer's `epoch_ms_max`.
    ///
    /// # Errors
    ///
    /// [`SettingError::close_request_range`] for `ms` outside 1 to 60,000; then
    /// [`SettingError::close_request_below_epochs`] for `ms` below twice `epoch_ms_max`.
    pub fn new(ms: u64, epoch_ms_max: u64) -> Result<Self, SettingError> {
        if !(Self::MS_MIN..=Self::MS_MAX).contains(&ms) {
            return Err(SettingError::close_request_range(ms));
        }
        if ms < epoch_ms_max.saturating_mul(2) {
            return Err(SettingError::close_request_below_epochs(ms, epoch_ms_max));
        }
        debug_assert!(ms <= Self::MS_MAX);
        Ok(Self { ms })
    }

    /// The milliseconds.
    pub fn ms(&self) -> u64 {
        self.ms
    }

    /// The same in microseconds, the commit step's clock.
    pub fn micros(&self) -> u64 {
        // At most 60,000 ms, so the product stays far below u64.
        self.ms.saturating_mul(1_000)
    }
}
