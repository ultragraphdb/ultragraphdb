/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::Readiness;

/// A role not ready, listening from its start: one that binds no address before `/health` may answer.
impl Default for Readiness {
    fn default() -> Self {
        Self {
            ready: Arc::new(AtomicBool::new(false)),
            listening: Arc::new(AtomicBool::new(true)),
        }
    }
}
