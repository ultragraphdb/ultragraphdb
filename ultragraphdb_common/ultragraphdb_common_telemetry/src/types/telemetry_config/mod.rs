/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::log_format::LogFormat;

/// What [`init`](crate::init) installs: the level filter, the output format, and where spans go.
///
/// Every value comes from the caller. The crate reads no environment variable for them, so a binary decides
/// whether `RUST_LOG` or a flag supplies the filter.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TelemetryConfig {
    filter: String,
    format: LogFormat,
    otlp_endpoint: Option<String>,
}

impl TelemetryConfig {
    /// Events filtered by `filter`, an `RUST_LOG`-style directive such as `info,ultragraphdb_store_kv=debug`,
    /// written in `format`; no spans exported.
    pub fn new(filter: impl Into<String>, format: LogFormat) -> Self {
        Self {
            filter: filter.into(),
            format,
            otlp_endpoint: None,
        }
    }

    /// The config with spans exported over OTLP gRPC to `endpoint`, e.g. `http://localhost:4317`.
    pub fn with_otlp_endpoint(self, endpoint: impl Into<String>) -> Self {
        Self {
            otlp_endpoint: Some(endpoint.into()),
            ..self
        }
    }

    /// The level filter directive.
    pub fn filter(&self) -> &str {
        &self.filter
    }

    /// The output format.
    pub fn format(&self) -> LogFormat {
        self.format
    }

    /// The OTLP endpoint spans are exported to, if any.
    pub fn otlp_endpoint(&self) -> Option<&str> {
        self.otlp_endpoint.as_deref()
    }
}
