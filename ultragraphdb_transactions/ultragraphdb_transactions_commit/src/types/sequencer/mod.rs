/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The commit versions: each admitted transaction's is the larger of the previous version plus 2 and twice the commit
/// step's clock in microseconds, read once per round, so versions are even and strictly increase, and a clock that
/// steps back never lowers them. The first transaction of a round takes `max(previous + 2, 2 × clock)` and each
/// later one the version before it plus 2, so a round's committed transaction k has `first + 2k`. An aborted
/// transaction takes none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sequencer {
    previous: u64,
    next: Option<u64>,
}

impl Sequencer {
    /// After `previous`, the newest commit version: the store's applied version at the open. An odd `previous`
    /// is followed by the even version above it.
    pub fn new(previous: u64) -> Self {
        Self {
            previous,
            next: None,
        }
    }

    /// Opens a round at `now_us`: the version its first admitted transaction takes. `None` once versions are
    /// exhausted, past `u64::MAX`.
    pub fn open_round(&mut self, now_us: u64) -> Option<u64> {
        // The smallest even version above the previous: previous + 2 for an even one.
        let after = (self.previous | 1).checked_add(1);
        self.next = match (after, now_us.checked_mul(2)) {
            (Some(after), Some(clock)) => Some(after.max(clock)),
            _ => None,
        };
        self.next
    }

    /// The version of the round's next admitted transaction, which becomes the previous version. `None` when no round
    /// is open or versions are exhausted.
    pub fn take(&mut self) -> Option<u64> {
        let version = self.next?;
        self.previous = version;
        self.next = version.checked_add(2);
        Some(version)
    }

    /// The newest version given, or the one the sequencer was opened after.
    pub fn previous(&self) -> u64 {
        self.previous
    }
}
