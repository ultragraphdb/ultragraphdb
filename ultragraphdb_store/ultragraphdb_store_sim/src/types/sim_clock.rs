/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Simulated time in nanoseconds, which advances only when the simulator moves it: a slow operation advances it
/// by its delay. Clones share one clock.
#[derive(Debug, Clone, Default)]
pub struct SimClock {
    nanos: Arc<AtomicU64>,
}

impl SimClock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn now(&self) -> u64 {
        self.nanos.load(Ordering::SeqCst)
    }

    pub fn advance(&self, nanos: u64) {
        self.nanos.fetch_add(nanos, Ordering::SeqCst);
    }
}
