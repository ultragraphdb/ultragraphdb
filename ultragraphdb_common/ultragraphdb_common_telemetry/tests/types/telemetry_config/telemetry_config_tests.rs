/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_telemetry::{LogFormat, TelemetryConfig};

#[test]
fn test_new_exports_no_spans() {
    let config = TelemetryConfig::new("info,ultragraphdb_store_kv=debug", LogFormat::Plain);
    assert_eq!(config.filter(), "info,ultragraphdb_store_kv=debug");
    assert_eq!(config.format(), LogFormat::Plain);
    assert_eq!(config.otlp_endpoint(), None);
}

#[test]
fn test_with_otlp_endpoint_keeps_the_rest() {
    let config =
        TelemetryConfig::new("warn", LogFormat::Json).with_otlp_endpoint("http://localhost:4317");
    assert_eq!(config.otlp_endpoint(), Some("http://localhost:4317"));
    assert_eq!(
        (config.filter(), config.format()),
        ("warn", LogFormat::Json)
    );
}
