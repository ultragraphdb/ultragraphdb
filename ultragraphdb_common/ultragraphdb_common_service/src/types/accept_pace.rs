/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt::Display;
use std::future::Future;
use std::time::Duration;

/// The pause after the first failed accept of a run, which doubles with each failure after it.
pub const ACCEPT_PAUSE_FIRST: Duration = Duration::from_millis(5);

/// The longest pause between two failed accepts. A failure that lasts, as running out of file descriptors (`EMFILE`) does,
/// costs one accept a second; one that passes, a connection closing and freeing a descriptor, is seen within that second. Go's
/// `net/http` server pauses its accept loop the same way, from 5 ms up to 1 s.
pub const ACCEPT_PAUSE_MAX: Duration = Duration::from_secs(1);

/// How an accept loop paces itself through failed accepts: each pauses before the next try, the pause doubling from
/// [`ACCEPT_PAUSE_FIRST`] up to [`ACCEPT_PAUSE_MAX`], and an accepted connection ends the run of failures. Every role's accept
/// loop takes one, so a listener that keeps failing tries once a second and warns once (Ultra rule 2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AcceptPace {
    failures: u32,
    pause: Duration,
}

impl AcceptPace {
    /// One more failed accept, and the pause before the next try.
    #[must_use]
    pub fn failed(&mut self) -> Duration {
        self.failures = self.failures.saturating_add(1);
        self.pause = if self.failures == 1 {
            ACCEPT_PAUSE_FIRST
        } else {
            self.pause.saturating_mul(2).min(ACCEPT_PAUSE_MAX)
        };
        self.pause
    }

    /// An accepted connection, which ends the run of failures: how many it ended.
    #[must_use]
    pub fn accepted(&mut self) -> u32 {
        let failures = self.failures;
        *self = Self::default();
        failures
    }

    /// The failed accepts in a row so far.
    #[must_use]
    pub fn failures(&self) -> u32 {
        self.failures
    }

    /// The next connection `accept` gives on the listener of `connections`, each failure pausing as [`Self::failed`] says. The
    /// warning is written once a run, at its first failure, and its end once, at the accept that ends it; the failures between
    /// are debug events.
    ///
    /// The loop is a step of the caller's event loop: it ends at the next accepted connection, and while the listener fails it
    /// tries once a pause. Dropping the future at its pause or its accept leaves the failures counted so far.
    pub async fn accept<T, E, F>(
        &mut self,
        connections: &'static str,
        mut accept: impl FnMut() -> F,
    ) -> T
    where
        F: Future<Output = Result<T, E>>,
        E: Display,
    {
        loop {
            match accept().await {
                Ok(accepted) => {
                    let failures = self.accepted();
                    if failures > 0 {
                        tracing::info!(connections, failures, "connections are accepted again");
                    }
                    return accepted;
                }
                Err(error) => {
                    let pause = self.failed();
                    let pause_ms = pause.as_millis();
                    if self.failures == 1 {
                        tracing::warn!(connections, %error, pause_ms, "a connection was not accepted: pausing before each next try until one is");
                    } else {
                        tracing::debug!(connections, %error, failures = self.failures, pause_ms, "a connection was not accepted");
                    }
                    compio::time::sleep(pause).await;
                }
            }
        }
    }
}
