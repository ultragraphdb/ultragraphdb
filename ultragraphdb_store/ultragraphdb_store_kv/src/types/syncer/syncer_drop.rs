/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::syncer::Syncer;

impl Drop for Syncer {
    /// Stops the thread and waits for it, so that a store reopened at once finds its directory released.
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(handle) = self.handle.take() {
            // A panic of the sync thread was reported where it happened; the store is closing either way.
            drop(handle.join());
        }
    }
}
