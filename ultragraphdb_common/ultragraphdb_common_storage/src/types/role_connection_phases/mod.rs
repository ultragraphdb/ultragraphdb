/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use ultragraphdb_common_service::PhaseWindow;

use crate::utils::micros_since::micros_since;

/// How long a request takes on a partition's role connection, from its session reading it to its first answer being taken by the
/// connection to be written, over a window ([`PhaseWindow`]) logged once it is due as `role connection phases`. Beside the frontend's
/// `one-frame write phases`, it leaves the two transits and the frontend's reading of the answer as the rest of a relay. A clone
/// shares the window.
///
/// The sessions of one partition's role address and its core run on one thread, so `Rc` lets them all hold the window and
/// `RefCell` lets each add to it; a borrow lasts one record or one report, never across an await. Data plane: recording
/// allocates nothing.
#[derive(Debug, Clone)]
pub struct RoleConnectionPhases {
    window: Rc<RefCell<PhaseWindow<1>>>,
    origin: Instant,
}

impl RoleConnectionPhases {
    /// A window holding nothing, its times counted from `origin`.
    #[must_use]
    pub fn new(origin: Instant) -> Self {
        Self {
            window: Rc::new(RefCell::new(PhaseWindow::new())),
            origin,
        }
    }

    /// Now, in microseconds since the origin.
    #[must_use]
    pub fn now_us(&self) -> u64 {
        micros_since(self.origin)
    }

    /// Counts a request read at `read_us` whose first answer was taken at `answered_us`.
    pub fn record(&self, read_us: u64, answered_us: u64) {
        self.window.borrow_mut().record(
            read_us,
            answered_us,
            [answered_us.saturating_sub(read_us)],
        );
    }

    /// The requests counted in the window now.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.window.borrow().count()
    }

    /// Logs the window of `partition` once it is due, and starts a new one: a report's allocation, once a second at most.
    pub fn report(&self, partition: u32) {
        let window = {
            let mut window = self.window.borrow_mut();
            if !window.is_due(self.now_us()) {
                return;
            }
            std::mem::take(&mut *window)
        };
        let (p50, p99, mean) = window.figures(0);
        tracing::info!(
            partition,
            requests = window.count(),
            window_us = window.window_us(),
            read_to_answer_p50_us = p50,
            read_to_answer_p99_us = p99,
            read_to_answer_mean_us = mean,
            "role connection phases"
        );
    }
}
