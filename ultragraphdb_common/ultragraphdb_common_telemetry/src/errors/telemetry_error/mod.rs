/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod telemetry_error_display;
mod telemetry_error_error;

/// Why [`init`](crate::init) or [`subscriber`](crate::subscriber) installed nothing.
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TelemetryError(pub TelemetryErrorEnum);

/// The classification of a [`TelemetryError`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TelemetryErrorEnum {
    /// The level filter is not a directive `tracing-subscriber`'s `EnvFilter` parses: the directive and the
    /// parser's reason.
    Filter { directive: String, reason: String },
    /// An OTLP endpoint is set and the caller is not inside a multi-threaded Tokio runtime, which the gRPC exporter
    /// runs its connection on while the batch processor's thread waits for it.
    Runtime,
    /// The OTLP span exporter refused its configuration: the endpoint and the exporter's reason.
    Exporter { endpoint: String, reason: String },
    /// A global subscriber or `log` logger is already installed in this process: the installer's reason.
    Installed(String),
}

impl TelemetryError {
    #[allow(non_snake_case)]
    pub fn Filter(directive: String, reason: String) -> Self {
        Self(TelemetryErrorEnum::Filter { directive, reason })
    }

    #[allow(non_snake_case)]
    pub fn Runtime() -> Self {
        Self(TelemetryErrorEnum::Runtime)
    }

    #[allow(non_snake_case)]
    pub fn Exporter(endpoint: String, reason: String) -> Self {
        Self(TelemetryErrorEnum::Exporter { endpoint, reason })
    }

    #[allow(non_snake_case)]
    pub fn Installed(reason: String) -> Self {
        Self(TelemetryErrorEnum::Installed(reason))
    }
}
