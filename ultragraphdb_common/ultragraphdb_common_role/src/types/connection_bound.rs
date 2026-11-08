/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::types::held_connection::HeldConnection;

/// How many connections a [`crate::Server`] holds, against the most it allows (`PROTOCOL.md` §11.9). Shared by an `Arc` of an
/// atomic: each connection counts for itself across every core. The count orders no other memory, so `AcqRel` on each
/// change and `Acquire` on a read suffice, as any ordering would: the changes to one atomic fall in one order.
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
        HeldConnection::new(
            Arc::clone(&self.held),
            self.limit,
            (before as u64) < self.limit, // lossless: a `usize` is at most 64 bits
        )
    }
}
