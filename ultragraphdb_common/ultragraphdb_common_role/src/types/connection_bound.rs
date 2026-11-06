/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// How many connections a [`crate::Server`] holds, against the most it allows (`PROTOCOL.md` §11.9). Shared by an `Arc` of an
/// atomic: each connection counts for itself across every core.
#[derive(Debug, Clone)]
pub struct ConnectionBound {
    held: Arc<AtomicUsize>,
    limit: u64,
}

impl ConnectionBound {
    /// A bound of `limit` connections, none held yet.
    #[must_use]
    pub fn new(limit: u64) -> Self {
        Self {
            held: Arc::default(),
            limit,
        }
    }

    /// How many connections it holds now.
    #[must_use]
    pub fn held(&self) -> usize {
        self.held.load(Ordering::Acquire)
    }

    /// Counts one more, until what this returns is dropped.
    #[must_use]
    pub fn enter(&self) -> HeldConnection {
        let before = self.held.fetch_add(1, Ordering::AcqRel);
        HeldConnection {
            held: Arc::clone(&self.held),
            limit: self.limit,
            within: (before as u64) < self.limit,
        }
    }
}

/// One connection of a [`ConnectionBound`]'s count.
#[derive(Debug)]
pub struct HeldConnection {
    held: Arc<AtomicUsize>,
    limit: u64,
    within: bool,
}

impl HeldConnection {
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
