/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The snapshot a lookup of the query core reads: the closed epoch every partition it asks reads at, and how long, in
/// milliseconds, each may wait for that close.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookupAt {
    epoch: u64,
    deadline_ms: u32,
}

impl LookupAt {
    /// The snapshot at `epoch`, waiting at most `deadline_ms`.
    #[must_use]
    pub fn new(epoch: u64, deadline_ms: u32) -> Self {
        Self { epoch, deadline_ms }
    }

    /// The epoch.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// How long a partition may wait for the epoch's close.
    #[must_use]
    pub fn deadline_ms(&self) -> u32 {
        self.deadline_ms
    }
}
