/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use opentelemetry_otlp::WithExportConfig;
use tokio::runtime::{Handle, RuntimeFlavor};
use tracing_subscriber::util::SubscriberInitExt;

use crate::errors::telemetry_error::TelemetryError;
use crate::telemetry::subscriber::subscriber;
use crate::types::telemetry_config::TelemetryConfig;
use crate::types::telemetry_guard::TelemetryGuard;

/// Installs the process's global subscriber, once, at a binary's start: events filtered by the config's directive
/// and written to stderr in its format, and, when the config names an OTLP endpoint, spans exported to it over gRPC
/// through a batch processor. Events of crates logging through `log` reach the same subscriber. Hold the returned
/// guard until the process exits.
///
/// With an endpoint, call it inside a multi-threaded Tokio runtime and drop the guard inside the same runtime: the
/// exporter's connection runs on it. The exporter connects when it first exports, not here. It also reads the
/// `OTEL_EXPORTER_OTLP_*` headers, timeout and compression variables, as the OpenTelemetry specification has them;
/// the endpoint passed here takes precedence over theirs.
///
/// # Errors
///
/// [`TelemetryErrorEnum::Filter`](crate::TelemetryErrorEnum::Filter) when the directive does not parse,
/// [`Runtime`](crate::TelemetryErrorEnum::Runtime) for an endpoint outside a multi-threaded Tokio runtime,
/// [`Exporter`](crate::TelemetryErrorEnum::Exporter) when the exporter refuses the endpoint, and
/// [`Installed`](crate::TelemetryErrorEnum::Installed) when a global subscriber or `log` logger is already set.
/// Nothing is installed on any of them.
pub fn init(config: &TelemetryConfig) -> Result<TelemetryGuard, TelemetryError> {
    let exporter = config.otlp_endpoint().map(otlp_exporter).transpose()?;
    let (subscriber, guard) = subscriber(config, std::io::stderr, exporter)?;
    subscriber
        .try_init()
        .map_err(|e| TelemetryError::Installed(e.to_string()))?;
    Ok(guard)
}

/// The OTLP span exporter over tonic to `endpoint`. tonic spawns the channel's worker on the current Tokio runtime,
/// and panics outside one, so the runtime is checked first.
fn otlp_exporter(endpoint: &str) -> Result<opentelemetry_otlp::SpanExporter, TelemetryError> {
    let multi_threaded = Handle::try_current()
        .is_ok_and(|handle| handle.runtime_flavor() == RuntimeFlavor::MultiThread);
    if !multi_threaded {
        return Err(TelemetryError::Runtime());
    }
    opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .map_err(|e| TelemetryError::Exporter(endpoint.to_string(), e.to_string()))
}
