/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// One connection of a [`crate::ConnectionBound`]'s count, which says why its ordering suffices.
#[derive(Debug)]
pub struct HeldConnection {
    held: Arc<AtomicUsize>,
    limit: u64,
    within: bool,
}

impl HeldConnection {
    /// One connection counted in `held`, of a bound of `limit`, `within` it or past it.
    pub(crate) fn new(held: Arc<AtomicUsize>, limit: u64, within: bool) -> Self {
        Self {
            held,
            limit,
            within,
        }
    }

    /// Whether the connection lies within the bound.
    #[must_use]
    pub fn within(&self) -> bool {
        self.within
    }

    /// The bound.
    #[must_use]
    pub fn limit(&self) -> u64 {
        self.limit
    }
}

impl Drop for HeldConnection {
    fn drop(&mut self) {
        let _before = self.held.fetch_sub(1, Ordering::AcqRel);
    }
}
