/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_telemetry::{LogFormat, TelemetryConfig, TelemetryError, init};

/// One test, because the global subscriber can be installed once per process: every refusal installs nothing, so
/// the first success installs, and the next call is refused.
#[test]
fn test_init_refuses_without_installing_then_installs_once() {
    // The reasons are the dependencies' own, as tracing-subscriber 0.3.23, opentelemetry-otlp 0.33.1 and
    // tracing-core give them.
    let refused = init(&TelemetryConfig::new("ultragraphdb=loud", LogFormat::Plain));
    assert_eq!(
        refused.map(drop).unwrap_err().to_string(),
        "invalid log filter \"ultragraphdb=loud\": error parsing level filter: expected one of \"off\", \"error\", \"warn\", \"info\", \"debug\", \"trace\", or a number 0-5"
    );

    let endpoint =
        TelemetryConfig::new("info", LogFormat::Json).with_otlp_endpoint("http://127.0.0.1:4317");
    let outside = init(&endpoint);
    assert_eq!(outside.map(drop), Err(TelemetryError::Runtime()));

    let current_thread = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime");
    let inside_current = current_thread.block_on(async { init(&endpoint) });
    assert_eq!(inside_current.map(drop), Err(TelemetryError::Runtime()));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .build()
        .expect("a runtime");
    runtime.block_on(async {
        let bad = TelemetryConfig::new("info", LogFormat::Json).with_otlp_endpoint("not a uri");
        assert_eq!(
            init(&bad).map(drop),
            Err(TelemetryError::Exporter(
                "not a uri".to_string(),
                "invalid exporter configuration: endpoint: invalid endpoint 'https://not a uri': invalid URI: invalid uri character".to_string()
            ))
        );

        // The exporter connects when it first exports; no span is created here, so nothing is sent.
        let guard = init(&endpoint).expect("the first subscriber of the process");
        let again = init(&TelemetryConfig::new("info", LogFormat::Plain));
        assert_eq!(
            again.map(drop),
            Err(TelemetryError::Installed(
                "a global default trace dispatcher has already been set".to_string()
            ))
        );
        drop(guard);
    });
}
