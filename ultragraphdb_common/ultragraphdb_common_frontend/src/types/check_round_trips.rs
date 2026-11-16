/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use ultragraphdb_common_service::PhaseWindow;

/// The round trips of the Check frames an endpoint core's link sent, each from its send to its reply (design D12), over a
/// window ([`PhaseWindow`]) logged as `check round trips` once it is due, with the number of the core it counts. A clone shares
/// the window.
///
/// The link's tasks run on the core's one thread, so `Rc` lets them hold the window and `RefCell` lets the reader record
/// into it; a borrow lasts one record or one report, never across an await.
#[derive(Debug, Clone)]
pub struct CheckRoundTrips {
    window: Rc<RefCell<PhaseWindow<1>>>,
    origin: Instant,
    core: usize,
}

impl CheckRoundTrips {
    /// A window holding nothing, its times counted from `origin`.
    #[must_use]
    pub fn new(origin: Instant) -> Self {
        Self {
            window: Rc::new(RefCell::new(PhaseWindow::new())),
            origin,
            core: 0,
        }
    }

    /// The window of endpoint core `core`, which its log names.
    #[must_use]
    pub fn with_core(mut self, core: usize) -> Self {
        self.core = core;
        self
    }

    /// Counts a Check frame sent at `sent` and answered at `answered`.
    pub fn record(&self, sent: Instant, answered: Instant) {
        let (sent_us, answered_us) = (self.micros(sent), self.micros(answered));
        let trip = micros(answered.saturating_duration_since(sent));
        self.window
            .borrow_mut()
            .record(sent_us, answered_us, [trip]);
    }

    /// The Check frames counted in the window not yet reported.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.window.borrow().count()
    }

    /// The window's median, 99th percentile and mean in microseconds, as [`PhaseWindow::figures`] gives them.
    #[must_use]
    pub fn figures(&self) -> (u64, u64, u64) {
        self.window.borrow().figures(0)
    }

    /// Logs the window once it is due at `at`, and starts a new one.
    pub fn report(&self, at: Instant) {
        let window = {
            let mut window = self.window.borrow_mut();
            if !window.is_due(self.micros(at)) {
                return;
            }
            std::mem::take(&mut *window)
        };
        let (p50, p99, mean) = window.figures(0);
        tracing::info!(
            core = self.core,
            checks = window.count(),
            window_us = window.window_us(),
            round_trip_p50_us = p50,
            round_trip_p99_us = p99,
            round_trip_mean_us = mean,
            "check round trips"
        );
    }

    /// `at` in microseconds from the window's origin.
    fn micros(&self, at: Instant) -> u64 {
        micros(at.saturating_duration_since(self.origin))
    }
}

/// `duration` in microseconds, `u64::MAX` past 584,942 years.
fn micros(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}
