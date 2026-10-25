/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::{Duration, Instant};

use crate::traits::clock::Clock;

/// The wall clock from the moment it is made: what a run is timed by.
#[derive(Debug, Clone, Copy)]
pub struct WallClock {
    start: Instant,
}

impl WallClock {
    #[allow(
        clippy::disallowed_methods,
        reason = "a benchmark times its run by the wall clock; nothing in the storage crates reads it"
    )]
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Default for WallClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for WallClock {
    #[allow(
        clippy::disallowed_methods,
        reason = "a benchmark times its run by the wall clock; nothing in the storage crates reads it"
    )]
    fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}
