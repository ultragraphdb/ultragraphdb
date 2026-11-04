/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Whether the service was told to stop, which a role still working reads between its steps.
///
/// Shared by an `Arc` of an atomic: the service's core sets it once, and a role's cores read it (Ultra rule 9).
#[derive(Debug, Clone, Default)]
pub struct Stopping(Arc<AtomicBool>);

impl Stopping {
    /// Whether the service was told to stop.
    #[must_use]
    pub fn is_stopping(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    /// Tells the role the service stops.
    pub fn set(&self) {
        self.0.store(true, Ordering::Release);
    }
}
