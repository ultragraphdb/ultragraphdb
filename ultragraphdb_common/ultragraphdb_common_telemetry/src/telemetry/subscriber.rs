/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use opentelemetry::trace::TracerProvider;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{BatchSpanProcessor, SdkTracerProvider, SpanExporter};
use tracing::Subscriber;
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::{Layer, SubscriberExt};
use tracing_subscriber::{EnvFilter, Registry, fmt};
use ultragraphdb_common_version::PRODUCT_NAME;

use crate::errors::telemetry_error::TelemetryError;
use crate::types::log_format::LogFormat;
use crate::types::telemetry_config::TelemetryConfig;
use crate::types::telemetry_guard::TelemetryGuard;

/// The targets of the span exporter's own stack. Their spans are kept from the exporter, because exporting them
/// creates more of them: at `trace`, every export would feed the next.
const EXPORTER_STACK: [&str; 8] = [
    "h2",
    "hyper",
    "hyper_util",
    "opentelemetry",
    "opentelemetry_otlp",
    "opentelemetry_sdk",
    "tonic",
    "tower",
];

/// The subscriber [`init`](crate::init) installs, over `writer` and `exporter` rather than stderr and the
/// config's OTLP endpoint, which this function does not read: events filtered by the config's directive and written
/// to `writer` in its format, and, when `exporter` is given, every span the filter passes exported through a batch
/// processor, under the service name `ultragraphdb`. A test installs it on its own thread with
/// `tracing::subscriber::with_default`.
///
/// # Errors
///
/// [`TelemetryErrorEnum::Filter`](crate::TelemetryErrorEnum::Filter) when the directive does not parse.
pub fn subscriber<W, E>(
    config: &TelemetryConfig,
    writer: W,
    exporter: Option<E>,
) -> Result<(impl Subscriber + Send + Sync + 'static, TelemetryGuard), TelemetryError>
where
    W: for<'w> MakeWriter<'w> + Send + Sync + 'static,
    E: SpanExporter + 'static,
{
    let filter = EnvFilter::try_new(config.filter())
        .map_err(|e| TelemetryError::Filter(config.filter().to_string(), e.to_string()))?;
    let (plain, json) = match config.format() {
        LogFormat::Plain => (
            Some(fmt::layer().with_ansi(false).with_writer(writer)),
            None,
        ),
        LogFormat::Json => (None, Some(fmt::layer().json().with_writer(writer))),
    };
    let (otel, provider) = match exporter {
        Some(exporter) => {
            let provider = SdkTracerProvider::builder()
                .with_span_processor(BatchSpanProcessor::builder(exporter).build())
                .with_resource(Resource::builder().with_service_name(PRODUCT_NAME).build())
                .build();
            let layer = tracing_opentelemetry::layer()
                .with_tracer(provider.tracer(PRODUCT_NAME))
                .with_filter(exporter_stack_off());
            (Some(layer), Some(provider))
        }
        None => (None, None),
    };
    let subscriber = Registry::default()
        .with(filter)
        .with(plain)
        .with(json)
        .with(otel);
    Ok((subscriber, TelemetryGuard::new(provider)))
}

/// Every target but the exporter's own stack.
fn exporter_stack_off() -> Targets {
    EXPORTER_STACK.iter().fold(
        Targets::new().with_default(LevelFilter::TRACE),
        |targets, target| targets.with_target(*target, LevelFilter::OFF),
    )
}
