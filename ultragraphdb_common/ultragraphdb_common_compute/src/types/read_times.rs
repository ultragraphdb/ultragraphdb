/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

/// The times a read keeps its versions by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadTimes {
    scan_stall: Duration,
    read_retain: Duration,
}

impl ReadTimes {
    /// `storage.scan_stall_ms` and `storage.read_retain_ms`.
    #[must_use]
    pub fn new(scan_stall: Duration, read_retain: Duration) -> Self {
        Self {
            scan_stall,
            read_retain,
        }
    }

    /// How long after a read begins it starts holding a version it chose as it began, well within what a partition retains between
    /// Scans.
    pub(crate) fn pin_after(&self) -> Duration {
        self.read_retain / 4
    }

    /// How often a held version's Scan takes a frame, well within what a partition waits on a stream.
    pub(crate) fn pin_every(&self) -> Duration {
        self.scan_stall / 3
    }

    /// How long a peer may leave compute's request unanswered past any deadline the request names: twice `scan_stall_ms`, since a
    /// partition refuses a Scan with no frame able to leave once that has passed. A peer silent past it is hung.
    pub(crate) fn answer_wait(&self) -> Duration {
        self.scan_stall.saturating_mul(2)
    }
}
