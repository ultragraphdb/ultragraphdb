/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use ultragraphdb_common_service::PhaseWindow;

use crate::types::client_session::now;

/// The phases of a one-frame write on the frontend, in its window's order: from its frame taken to its relay to its partition,
/// the checks (the gate, the token, the session's owner check, the terms and the route); from its relay to its partition's
/// answer; and from its frame taken to that answer, which then leaves for the client.
pub const ONE_FRAME_PHASES: [&str; 3] = ["checks", "partition", "total"];

/// The phases of the DirectWrites and ConditionalWrites an endpoint core relays, over a window ([`PhaseWindow`]), logged once it is
/// due. A clone shares the window.
///
/// The sessions of one endpoint core run on its one thread and each records a write once its answer arrives, so `Rc` lets them all
/// hold the window, and `RefCell` lets each add to it; a borrow lasts one record or one report, never across an await.
#[derive(Debug, Clone)]
pub struct OneFramePhases {
    window: Rc<RefCell<PhaseWindow<{ ONE_FRAME_PHASES.len() }>>>,
    origin: Instant,
}

impl Default for OneFramePhases {
    fn default() -> Self {
        Self::new(now())
    }
}

impl OneFramePhases {
    /// A window holding nothing, its times counted from `origin`.
    #[must_use]
    pub fn new(origin: Instant) -> Self {
        Self {
            window: Rc::new(RefCell::new(PhaseWindow::new())),
            origin,
        }
    }

    /// Counts a write taken at `began`, relayed at `relayed` and answered at `answered`.
    pub fn record(&self, began: Instant, relayed: Instant, answered: Instant) {
        let us = |from: Instant, to: Instant| micros(to.saturating_duration_since(from));
        let phases = [
            us(began, relayed),
            us(relayed, answered),
            us(began, answered),
        ];
        let (began_us, answered_us) = (us(self.origin, began), us(self.origin, answered));
        self.window
            .borrow_mut()
            .record(began_us, answered_us, phases);
    }

    /// The window, if it is due at `at`, a new one taking its place: a report's allocation, once a second at most.
    #[must_use]
    pub fn take_due(&self, at: Instant) -> Option<PhaseWindow<{ ONE_FRAME_PHASES.len() }>> {
        let mut window = self.window.borrow_mut();
        if !window.is_due(micros(at.saturating_duration_since(self.origin))) {
            return None;
        }
        Some(std::mem::take(&mut *window))
    }

    /// Logs the window once it is due at `at`, and starts a new one.
    pub fn report(&self, at: Instant) {
        let Some(window) = self.take_due(at) else {
            return;
        };
        let [checks, partition, total] = [0, 1, 2].map(|index| window.figures(index));
        tracing::info!(
            writes = window.count(),
            window_us = window.window_us(),
            checks_p50_us = checks.0,
            checks_p99_us = checks.1,
            checks_mean_us = checks.2,
            partition_p50_us = partition.0,
            partition_p99_us = partition.1,
            partition_mean_us = partition.2,
            total_p50_us = total.0,
            total_p99_us = total.1,
            total_mean_us = total.2,
            "one-frame write phases"
        );
    }
}

/// `duration` in microseconds, `u64::MAX` past 584,942 years.
fn micros(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}
