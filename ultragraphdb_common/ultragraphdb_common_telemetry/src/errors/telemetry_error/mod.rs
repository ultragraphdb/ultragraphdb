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
    Filter {
        /// The directive as configured.
        directive: String,
        /// The parser's reason.
        reason: String,
    },
    /// The OTLP span exporter refused its configuration: the endpoint and the exporter's reason.
    Exporter {
        /// The endpoint as configured.
        endpoint: String,
        /// The exporter's reason.
        reason: String,
    },
    /// A global subscriber or `log` logger is already installed in this process: the installer's reason.
    Installed(String),
}

impl TelemetryError {
    /// [`TelemetryErrorEnum::Filter`] of `directive` for `reason`.
    #[must_use]
    pub fn filter(directive: String, reason: String) -> Self {
        Self(TelemetryErrorEnum::Filter { directive, reason })
    }

    /// [`TelemetryErrorEnum::Exporter`] of `endpoint` for `reason`.
    #[must_use]
    pub fn exporter(endpoint: String, reason: String) -> Self {
        Self(TelemetryErrorEnum::Exporter { endpoint, reason })
    }

    /// [`TelemetryErrorEnum::Installed`] for the installer's `reason`.
    #[must_use]
    pub fn installed(reason: String) -> Self {
        Self(TelemetryErrorEnum::Installed(reason))
    }
}
