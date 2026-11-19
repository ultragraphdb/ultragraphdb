/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::Future;
use std::time::{Duration, Instant};

use crate::traits::serve_clock::ServeClock;

/// The wall clock a role's server reads and waits on, the one a running process has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WallClock;

impl ServeClock for WallClock {
    fn now(&self) -> Instant {
        crate::utils::serve::now()
    }

    fn sleep(&self, wait: Duration) -> impl Future<Output = ()> {
        compio::time::sleep(wait)
    }
}
