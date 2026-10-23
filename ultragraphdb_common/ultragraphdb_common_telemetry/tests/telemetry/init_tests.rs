/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_telemetry::{LogFormat, TelemetryConfig, TelemetryErrorEnum, init};

/// One test, because the global subscriber can be installed once per process: every refusal installs nothing, so
/// the first success installs, and the next call is refused.
#[test]
fn test_init_refuses_without_installing_then_installs_once() {
    let refused = init(&TelemetryConfig::new("ultragraphdb=loud", LogFormat::Plain));
    assert!(matches!(
        refused.map_err(|e| e.0),
        Err(TelemetryErrorEnum::Filter { .. })
    ));

    let endpoint =
        TelemetryConfig::new("info", LogFormat::Json).with_otlp_endpoint("http://127.0.0.1:4317");
    let outside = init(&endpoint);
    assert!(matches!(
        outside.map_err(|e| e.0),
        Err(TelemetryErrorEnum::Runtime)
    ));

    let current_thread = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime");
    let inside_current = current_thread.block_on(async { init(&endpoint) });
    assert!(matches!(
        inside_current.map_err(|e| e.0),
        Err(TelemetryErrorEnum::Runtime)
    ));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .build()
        .expect("a runtime");
    runtime.block_on(async {
        let bad = TelemetryConfig::new("info", LogFormat::Json).with_otlp_endpoint("not a uri");
        assert!(matches!(
            init(&bad).map_err(|e| e.0),
            Err(TelemetryErrorEnum::Exporter { endpoint, .. }) if endpoint == "not a uri"
        ));

        // The exporter connects when it first exports; no span is created here, so nothing is sent.
        let guard = init(&endpoint).expect("the first subscriber of the process");
        let again = init(&TelemetryConfig::new("info", LogFormat::Plain));
        assert!(matches!(
            again.map_err(|e| e.0),
            Err(TelemetryErrorEnum::Installed(_))
        ));
        drop(guard);
    });
}
