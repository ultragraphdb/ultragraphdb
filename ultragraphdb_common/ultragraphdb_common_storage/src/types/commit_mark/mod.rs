/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::log_io_totals::LogIoTotals;

/// Where a partition's core stood when a commit reached a step of its way: the core's clock, what its log had spent in its I/O,
/// and how long the core had waited on its inbox, each since the core began. Two marks of one commit give its phases between
/// them. 56 bytes, within a cache line, so it is copied.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CommitMark {
    at_us: u64,
    log: LogIoTotals,
    waited_us: u64,
}

impl CommitMark {
    /// The mark at `at_us`, the log having spent `log` and the core having waited `waited_us` on its inbox.
    #[must_use]
    pub fn new(at_us: u64, log: LogIoTotals, waited_us: u64) -> Self {
        Self {
            at_us,
            log,
            waited_us,
        }
    }

    /// The core's clock.
    #[must_use]
    pub fn at_us(&self) -> u64 {
        self.at_us
    }

    /// What its log had spent in its I/O.
    #[must_use]
    pub fn log(&self) -> &LogIoTotals {
        &self.log
    }

    /// How long the core had waited on its inbox, its runtime's other tasks, its connections', running meanwhile.
    #[must_use]
    pub fn waited_us(&self) -> u64 {
        self.waited_us
    }
}
