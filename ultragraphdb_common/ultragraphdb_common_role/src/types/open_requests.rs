/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use ultragraphdb_common_service::Stopping;

use crate::types::open_frame::OpenFrame;

/// What the connections of a [`crate::Server`] hold open, for a stop to wait on: every frame taken and not yet answered on the
/// wire. Once the service is stopping, a connection refuses each new request as unavailable.
///
/// Shared by an `Arc` of an atomic: each connection counts for itself, and whoever stops reads the sum (Ultra rule 9).
/// The count orders no other memory, so `AcqRel` on each change and `Acquire` on a read suffice, as any ordering would:
/// the changes to one atomic fall in one order.
#[derive(Debug, Clone, Default)]
pub struct OpenRequests {
    open: Arc<AtomicUsize>,
    stopping: Stopping,
}

impl OpenRequests {
    /// Requests counted from here on, refused once `stopping` says so.
    #[must_use]
    pub fn new(stopping: Stopping) -> Self {
        Self {
            open: Arc::default(),
            stopping,
        }
    }

    /// How many are open now.
    #[must_use]
    pub fn count(&self) -> usize {
        self.open.load(Ordering::Acquire)
    }

    /// Whether a new request is refused.
    #[must_use]
    pub fn is_stopping(&self) -> bool {
        self.stopping.is_stopping()
    }

    /// Counts one more, until what this returns is dropped: a frame a connection has taken, or a request a session has relayed
    /// and not yet answered.
    #[must_use]
    pub fn enter(&self) -> OpenFrame {
        OpenFrame::new(Arc::clone(&self.open))
    }
}
