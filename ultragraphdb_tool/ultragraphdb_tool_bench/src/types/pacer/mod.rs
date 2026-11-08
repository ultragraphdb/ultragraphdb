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
        self.release(self.due_by(clock.elapsed()))
    }

    /// The commits that came due since the last call and before `end`, by `clock`: a caller looking only after `end`,
    /// as a writer held by a slow sync does, still takes every commit due before it, and none due at it or after.
    pub fn take_due_before<C: Clock>(&mut self, clock: &C, end: Duration) -> u64 {
        self.release(self.due_by(clock.elapsed()).min(self.due_before(end)))
    }

    /// How many commits are due before `end`, those due at `end` left out: commit `n` is when `n / rate` is below it.
    pub fn due_before(&self, end: Duration) -> u64 {
        // Less one, so a commit due at `end` exactly is left out; saturating as in `due_by`.
        let scaled = end
            .as_nanos()
            .saturating_mul(u128::from(self.rate))
            .saturating_sub(1);
        u64::try_from(scaled / 1_000_000_000).unwrap_or(u64::MAX)
    }

    /// How many commits are due by `elapsed`, its own included.
    fn due_by(&self, elapsed: Duration) -> u64 {
        // Saturating: past u64::MAX commits, which no run reaches, the count stays there rather than wrapping.
        let scaled = elapsed.as_nanos().saturating_mul(u128::from(self.rate));
        u64::try_from(scaled / 1_000_000_000).unwrap_or(u64::MAX)
    }

    /// Releases the commits up to `due`: those not released before.
    fn release(&mut self, due: u64) -> u64 {
        let fresh = due.saturating_sub(self.released);
        self.released = self.released.max(due);
        debug_assert!(self.released >= due);
        fresh
    }

    /// When the next commit is due, from the run's start; never for a rate of zero.
    pub fn next_due(&self) -> Duration {
        if self.rate == 0 {
            return Duration::MAX;
        }
        // Rounded up: the first nanosecond at which `take_due` counts the commit.
        let nanos =
            ((u128::from(self.released) + 1) * 1_000_000_000).div_ceil(u128::from(self.rate));
        Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
    }

    /// Every commit released so far.
    pub fn released(&self) -> u64 {
        self.released
    }
}
