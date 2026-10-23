/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use super::TelemetryGuard;

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.provider.take() {
            // Shutting the provider down exports what its batch processor still queues, then shuts the exporter
            // down. A failure has nowhere to go: the subscriber that would report it is the one being torn down.
            let _ = provider.shutdown();
        }
    }
}
