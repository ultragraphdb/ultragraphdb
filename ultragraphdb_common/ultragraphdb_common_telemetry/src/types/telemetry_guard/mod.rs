/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod telemetry_guard_drop;

use opentelemetry_sdk::trace::SdkTracerProvider;

/// Keeps span export running: dropping it exports the spans still queued and shuts the exporter down. A binary
/// holds it until it exits, and drops it inside the Tokio runtime the exporter was built in, which the final export
/// runs on. Without an OTLP endpoint it holds nothing.
#[derive(Debug)]
#[must_use = "dropping the guard shuts span export down"]
pub struct TelemetryGuard {
    provider: Option<SdkTracerProvider>,
}

impl TelemetryGuard {
    pub(crate) fn new(provider: Option<SdkTracerProvider>) -> Self {
        Self { provider }
    }
}
