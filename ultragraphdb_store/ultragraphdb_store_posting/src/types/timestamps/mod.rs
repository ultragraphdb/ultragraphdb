/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

/// The reads open on a store, by read version, and the discard timestamp they allow: the oldest open read bounds it.
/// Commit versions are the commit step's to give, and a rollup's follow from the commits it folds, so this gives none.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Timestamps {
    open: BTreeMap<u64, usize>,
}

impl Timestamps {
    /// Records a read open at `ts`.
    pub fn open_read(&mut self, ts: u64) {
        *self.open.entry(ts).or_default() += 1;
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
