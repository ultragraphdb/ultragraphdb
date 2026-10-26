/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

/// The local timestamp counter of stage 2, which stands in for Zero, and the reads it has open. Every commit and
/// every rollup takes a fresh timestamp from it; a read at a timestamp moves the counter past it, so no commit can
/// land at or below a timestamp a read has used. The oldest open read bounds the discard timestamp.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Timestamps {
    next: u64,
    open: BTreeMap<u64, usize>,
}

impl Timestamps {
    /// A counter for a store whose newest commit is `applied_ts`. Its first timestamp is `applied_ts + 2`: a rollup
    /// writes at most one above the newest commit it read, so the store may hold a version at `applied_ts + 1`, and
    /// a commit there would replace it.
    pub fn new(applied_ts: u64) -> Self {
        Self {
            next: applied_ts.saturating_add(2),
            open: BTreeMap::new(),
        }
    }

    /// A timestamp no one has had.
    pub fn fresh(&mut self) -> u64 {
        let ts = self.next;
        self.next += 1;
        ts
    }

    /// The timestamp [`Timestamps::fresh`] gives next.
    pub fn peek(&self) -> u64 {
        self.next
    }

    /// Moves the counter past `ts`, which a read uses. A read at `u64::MAX` reads everything, and no commit can land
    /// above it, so it leaves the counter where it is: moving it there would overflow the next commit's timestamp.
    pub fn observe(&mut self, ts: u64) {
        if ts < u64::MAX {
            self.next = self.next.max(ts + 1);
        }
    }

    /// Records a read open at `ts`.
    pub fn open_read(&mut self, ts: u64) {
        *self.open.entry(ts).or_default() += 1;
        self.observe(ts);
    }

    /// Records the end of a read opened at `ts`.
    pub fn close_read(&mut self, ts: u64) {
        if let Some(count) = self.open.get_mut(&ts) {
            *count -= 1;
            if *count == 0 {
                self.open.remove(&ts);
            }
        }
    }

    /// The timestamp of the oldest open read.
    pub fn oldest_open_read(&self) -> Option<u64> {
        self.open.keys().next().copied()
    }

    /// The discard timestamp `requested` may become: at most the oldest open read.
    pub fn discard_bound(&self, requested: u64) -> u64 {
        self.oldest_open_read()
            .map_or(requested, |oldest| requested.min(oldest))
    }
}
