/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a binary calls once at start to make the workspace's `tracing` events and spans visible: [`init`]
//! installs the process's global subscriber from a [`TelemetryConfig`], with events written to stderr as plain
//! text or JSON lines, filtered by an `RUST_LOG`-style directive the caller passes, and, when the config names an
//! OTLP endpoint, spans exported over gRPC through a batch processor. The returned [`TelemetryGuard`] flushes the
//! exporter and shuts it down when it is dropped.
//!
//! Library crates never call this crate: they emit events with `tracing` and leave the subscriber to the binary.
//! [`subscriber`] builds what [`init`] installs over a writer and a span exporter of the caller's choosing, so a
//! test installs it on its own thread.
//!
//! ```
//! use ultragraphdb_common_telemetry::{LogFormat, TelemetryConfig, init};
//!
//! let config = TelemetryConfig::new("info,ultragraphdb_core_schema=debug", LogFormat::Json);
//! let _guard = init(&config).expect("the first subscriber of the process");
//! tracing::info!(predicate = "0-name", "Deleting schema for predicate");
//! ```

mod errors;
mod telemetry;
mod types;

pub use crate::errors::telemetry_error::{TelemetryError, TelemetryErrorEnum};
pub use crate::telemetry::init::init;
pub use crate::telemetry::subscriber::subscriber;
pub use crate::types::log_format::LogFormat;
pub use crate::types::telemetry_config::TelemetryConfig;
pub use crate::types::telemetry_guard::TelemetryGuard;
