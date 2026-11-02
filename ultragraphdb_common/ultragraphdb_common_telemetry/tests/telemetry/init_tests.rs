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

    for endpoint in ["https://127.0.0.1:4318/v1/traces", "127.0.0.1:4318", ""] {
        let tls = TelemetryConfig::new("info", LogFormat::Json).with_otlp_endpoint(endpoint);
        assert_eq!(
            init(&tls).map(drop),
            Err(TelemetryError::Exporter(
                endpoint.to_string(),
                "the exporter speaks plain HTTP; TLS is the collector's".to_string()
            ))
        );
    }

    let bad = TelemetryConfig::new("info", LogFormat::Json).with_otlp_endpoint("http://not a uri");
    assert_eq!(
        init(&bad).map(drop),
        Err(TelemetryError::Exporter(
            "http://not a uri".to_string(),
            "invalid exporter configuration: endpoint: invalid endpoint 'http://not a uri': invalid uri character".to_string()
        ))
    );

    // No async runtime around the call. The exporter connects when it first exports; no span is created here, so
    // nothing is sent.
    let endpoint = TelemetryConfig::new("info", LogFormat::Json)
        .with_otlp_endpoint("http://127.0.0.1:4318/v1/traces");
    let guard = init(&endpoint).expect("the first subscriber of the process");
    let again = init(&TelemetryConfig::new("info", LogFormat::Plain));
    assert_eq!(
        again.map(drop),
        Err(TelemetryError::Installed(
            "a global default trace dispatcher has already been set".to_string()
        ))
    );
    drop(guard);
}
