/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use crate::traits::clock::Clock;

/// An open-loop source at a fixed offered rate: commit `n`, counted from 1, is due at `n / rate` seconds after the
/// run's start, whether or not the apply thread has kept up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pacer {
    rate: u64,
    released: u64,
}

impl Pacer {
    /// A pacer offering `rate` commits a second.
    pub fn new(rate: u64) -> Self {
        Self { rate, released: 0 }
    }

    /// The commits that came due since the last call, by `clock`.
    pub fn take_due<C: Clock>(&mut self, clock: &C) -> u64 {
        let due = (clock.elapsed().as_nanos() * u128::from(self.rate) / 1_000_000_000) as u64;
        let fresh = due.saturating_sub(self.released);
        self.released = self.released.max(due);
        fresh
    }

    /// When the next commit is due, from the run's start; never for a rate of zero.
    pub fn next_due(&self) -> Duration {
        if self.rate == 0 {
            return Duration::MAX;
        }
        let nanos = u128::from(self.released + 1) * 1_000_000_000 / u128::from(self.rate);
        Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
    }

    /// Every commit released so far.
    pub fn released(&self) -> u64 {
        self.released
    }
}
