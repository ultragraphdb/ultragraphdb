/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use opentelemetry_otlp::{WithExportConfig, WithHttpConfig};
use tracing_subscriber::util::SubscriberInitExt;

use crate::errors::telemetry_error::TelemetryError;
use crate::telemetry::subscriber::subscriber;
use crate::types::plain_http_client::PlainHttpClient;
use crate::types::telemetry_config::TelemetryConfig;
use crate::types::telemetry_guard::TelemetryGuard;

/// How long one export request may take: OTLP's default export timeout.
const EXPORT_TIMEOUT: Duration = Duration::from_secs(10);

/// Installs the process's global subscriber, once, at a binary's start: events filtered by the config's directive
/// and written to stderr in its format, and, when the config names an OTLP endpoint, spans exported to it over
/// OTLP/HTTP with protobuf through a batch processor, by [`PlainHttpClient`] on the processor's own thread. Events
/// of crates logging through `log` reach the same subscriber. Hold the returned guard until the process exits.
///
/// No async runtime is needed. The exporter connects when it first exports, not here, and sends the
/// `OTEL_EXPORTER_OTLP_HEADERS` the OpenTelemetry specification names; each request takes at most 10 s.
///
/// # Errors
///
/// [`TelemetryErrorEnum::Filter`](crate::TelemetryErrorEnum::Filter) when the directive does not parse,
/// [`Exporter`](crate::TelemetryErrorEnum::Exporter) when the endpoint is not an `http://` URL the exporter
/// takes, and [`Installed`](crate::TelemetryErrorEnum::Installed) when a global subscriber or `log` logger is
/// already set. Nothing is installed on any of them.
pub fn init(config: &TelemetryConfig) -> Result<TelemetryGuard, TelemetryError> {
    let exporter = config.otlp_endpoint().map(otlp_exporter).transpose()?;
    let (subscriber, guard) = subscriber(config, std::io::stderr, exporter)?;
    subscriber
        .try_init()
        .map_err(|e| TelemetryError::Installed(e.to_string()))?;
    Ok(guard)
}

/// The OTLP/HTTP span exporter to `endpoint`, sending through [`PlainHttpClient`]. An `https://` endpoint is
/// refused here, as the client would refuse every request to it.
fn otlp_exporter(endpoint: &str) -> Result<opentelemetry_otlp::SpanExporter, TelemetryError> {
    let refused = |reason: String| TelemetryError::Exporter(endpoint.to_string(), reason);
    if !endpoint
        .get(..7)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("http://"))
    {
        return Err(refused(
            "the exporter speaks plain HTTP; TLS is the collector's".to_string(),
        ));
    }
    opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(endpoint)
        .with_http_client(PlainHttpClient::new(EXPORT_TIMEOUT))
        .build()
        .map_err(|e| refused(e.to_string()))
}
