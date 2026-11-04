/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transport_http_client::{HttpClientErrorEnum, HttpUrl};

/// A URL reads to its scheme, host and port.
#[test]
fn test_a_url_reads_its_parts() -> TestResult {
    let secure = HttpUrl::parse("https://config.svc:8443")?;
    assert!(secure.is_secure());
    assert_eq!((secure.host(), secure.port()), ("config.svc", 8443));
    assert_eq!(secure.as_str(), "https://config.svc:8443");
    assert!(!HttpUrl::parse("http://127.0.0.1:80")?.is_secure());
    Ok(())
}

/// Anything else is refused naming what was given, a path and a missing port among it.
#[test]
fn test_anything_else_is_refused_naming_it() -> TestResult {
    for wrong in [
        "config.svc:80",
        "ftp://h:1",
        "http://h",
        "http://:80",
        "http://h:port",
        "http://h:70000",
        "http://h:80/path",
        "http://h h:80",
    ] {
        let refused = HttpUrl::parse(wrong).err().context(wrong)?;
        assert!(
            matches!(refused.0, HttpClientErrorEnum::Url { .. }),
            "{wrong}"
        );
        assert!(refused.to_string().contains(wrong), "{wrong}");
    }
    Ok(())
}
