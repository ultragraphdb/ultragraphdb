/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use super::{TelemetryError, TelemetryErrorEnum};

impl fmt::Display for TelemetryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            TelemetryErrorEnum::Filter { directive, reason } => {
                write!(f, "invalid log filter {directive:?}: {reason}")
            }
            TelemetryErrorEnum::Runtime => write!(
                f,
                "an OTLP endpoint needs a multi-threaded Tokio runtime around the call to init"
            ),
            TelemetryErrorEnum::Exporter { endpoint, reason } => {
                write!(f, "invalid OTLP endpoint {endpoint:?}: {reason}")
            }
            TelemetryErrorEnum::Installed(reason) => {
                write!(f, "telemetry already installed: {reason}")
            }
        }
    }
}
