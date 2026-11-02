/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_telemetry::{LogFormat, TelemetryConfig, TelemetryError, init};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

/// One test, because the global subscriber can be installed once per process: every refusal installs nothing, so
/// the first success installs, and the next call is refused.
#[test]
fn test_init_refuses_without_installing_then_installs_once() -> TestResult {
    // The reasons are the dependencies' own, as tracing-subscriber 0.3.23, opentelemetry-otlp 0.33.1 and
    // tracing-core give them.
    let Err(refused) = init(&TelemetryConfig::new("ultragraphdb=loud", LogFormat::Plain)) else {
        return Err(TestError::new(
            "an unparsable directive installed a subscriber",
        ));
    };
    assert_eq!(
        refused.to_string(),
        "invalid log filter \"ultragraphdb=loud\": error parsing level filter: expected one of \"off\", \"error\", \"warn\", \"info\", \"debug\", \"trace\", or a number 0-5"
    );

    for endpoint in ["127.0.0.1:4318", "", "ftp://127.0.0.1:4318/v1/traces"] {
        let config = TelemetryConfig::new("info", LogFormat::Json).with_otlp_endpoint(endpoint);
        assert_eq!(
            init(&config).map(drop),
            Err(TelemetryError::exporter(
                endpoint.to_string(),
                "the endpoint is an http:// or https:// URL".to_string()
            ))
        );
    }

    // Bazel sets SSL_CERT_FILE to a path no file is at (BUILD.bazel), so an https:// endpoint finds no trust store;
    // the reason is the TLS crate's, around the operating system's.
    let https = TelemetryConfig::new("info", LogFormat::Json)
        .with_otlp_endpoint("https://127.0.0.1:4318/v1/traces");
    assert_eq!(
        init(&https).map(drop),
        Err(TelemetryError::exporter(
            "https://127.0.0.1:4318/v1/traces".to_string(),
            "cannot read the trust store \"/nonexistent/ultragraphdb/from-environment.pem\": No such file or directory (os error 2)".to_string()
        ))
    );

    let bad = TelemetryConfig::new("info", LogFormat::Json).with_otlp_endpoint("http://not a uri");
    assert_eq!(
        init(&bad).map(drop),
        Err(TelemetryError::exporter(
            "http://not a uri".to_string(),
            "invalid exporter configuration: endpoint: invalid endpoint 'http://not a uri': invalid uri character".to_string()
        ))
    );

    // No async runtime around the call. The exporter connects when it first exports; no span is created here, so
    // nothing is sent.
    let endpoint = TelemetryConfig::new("info", LogFormat::Json)
        .with_otlp_endpoint("http://127.0.0.1:4318/v1/traces");
    let guard = init(&endpoint).context("the first subscriber of the process")?;
    let again = init(&TelemetryConfig::new("info", LogFormat::Plain));
    assert_eq!(
        again.map(drop),
        Err(TelemetryError::installed(
            "a global default trace dispatcher has already been set".to_string()
        ))
    );
    drop(guard);
    Ok(())
}
