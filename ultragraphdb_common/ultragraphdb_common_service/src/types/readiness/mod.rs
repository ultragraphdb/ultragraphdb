/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod readiness_default;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Whether a role listens and is ready, which its role's work sets, `/health` answering the first and `/ready` the second.
///
/// A role that binds listeners from its start, the frontend, starts [`Readiness::binding`] and sets itself listening once
/// they are bound, so `/health` answering says a connection to its addresses is accepted; every other role starts
/// listening ([`Readiness::default`]). Shared by an `Arc` of atomics: the responder's core reads what a role's other core
/// sets, once at start and once at shutdown, so this is control plane and no request's path touches it (Ultra rule 9).
/// `Release` on a store and `Acquire` on a load suffice: a reader seeing a flag set sees what the setter wrote before.
#[derive(Debug, Clone)]
pub struct Readiness {
    ready: Arc<AtomicBool>,
    listening: Arc<AtomicBool>,
}

impl Readiness {
    /// A role neither ready nor listening yet, which sets itself listening once it has bound its addresses.
    #[must_use]
    pub fn binding() -> Self {
        Self {
            ready: Arc::new(AtomicBool::new(false)),
            listening: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Whether the role is ready.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    /// Sets whether the role is ready.
    pub fn set(&self, ready: bool) {
        self.ready.store(ready, Ordering::Release);
    }

    /// Whether the role has bound the addresses it serves from its start.
    #[must_use]
    pub fn is_listening(&self) -> bool {
        self.listening.load(Ordering::Acquire)
    }

    /// Says the role has bound the addresses it serves from its start.
    pub fn set_listening(&self) {
        self.listening.store(true, Ordering::Release);
    }
}
