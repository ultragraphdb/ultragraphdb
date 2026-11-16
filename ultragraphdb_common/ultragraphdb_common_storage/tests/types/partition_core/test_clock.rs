/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A core's clock in a test, which the test moves on: running from the runtime's clock, so time also passes by itself, or
//! stopped, so a timer of the core is due only once the test advances it there. The core and the test run on one thread, so the
//! advance is a cell they share.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ultragraphdb_common_storage::CoreClock;

/// See the module's documentation.
#[derive(Debug, Clone)]
pub(crate) struct TestClock {
    /// The runtime's instant the core began at, for a running clock; none for a stopped one.
    origin: Option<Instant>,
    advanced_us: Rc<Cell<u64>>,
}

impl TestClock {
    /// The clock of a core that began at `origin`, running with the runtime's, not yet advanced.
    pub(crate) fn running(origin: Instant) -> Self {
        Self {
            origin: Some(origin),
            advanced_us: Rc::new(Cell::new(0)),
        }
    }

    /// A clock at 0 that moves only when the test advances it.
    pub(crate) fn stopped() -> Self {
        Self {
            origin: None,
            advanced_us: Rc::new(Cell::new(0)),
        }
    }

    /// Moves the clock on by `by`, at once: the core's next turn reads it.
    pub(crate) fn advance(&self, by: Duration) {
        let by_us = u64::try_from(by.as_micros()).unwrap_or(u64::MAX);
        self.advanced_us
            .set(self.advanced_us.get().saturating_add(by_us));
    }
}

impl CoreClock for TestClock {
    fn now_us(&self) -> u64 {
        self.origin
            .map_or(0, |origin| origin.now_us())
            .saturating_add(self.advanced_us.get())
    }
}
