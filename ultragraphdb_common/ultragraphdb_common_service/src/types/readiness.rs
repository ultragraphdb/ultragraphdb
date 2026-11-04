/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Whether a role is ready, which its role's work sets and its `/ready` answers.
///
/// Shared by an `Arc` of an atomic: the responder's core reads what a role's other core sets, once at start and once at
/// shutdown, so this is control plane and no request's path touches it (Ultra rule 9).
#[derive(Debug, Clone, Default)]
pub struct Readiness(Arc<AtomicBool>);

impl Readiness {
    /// Whether the role is ready.
    pub fn is_ready(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    /// Sets whether the role is ready.
    pub fn set(&self, ready: bool) {
        self.0.store(ready, Ordering::Release);
    }
}
