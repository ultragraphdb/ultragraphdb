/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::Future;
use std::time::{Duration, Instant};

/// Where a [`crate::Server`] reads the time and waits for a connection's next deadline: the authentication timeout and, on a
/// client's connection, the idle limit (`PROTOCOL.md` §11.4, §11.9). The shell's own reads the wall clock
/// ([`crate::WallClock`]); a test's moves its time itself, so a limit of minutes is reached without a real-time wait.
pub trait ServeClock: Clone {
    /// The time now.
    fn now(&self) -> Instant;

    /// Waits `wait`, the time until a connection's next deadline as [`ServeClock::now`] read it. A dropped future waits no
    /// more.
    fn sleep(&self, wait: Duration) -> impl Future<Output = ()>;
}
