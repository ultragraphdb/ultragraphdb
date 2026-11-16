/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use ultragraphdb_tool_bdd::{TestError, TestResult, hides};
use ultragraphdb_transport_http_server::{HttpLimits, ParsedHead, parse_head};

const KEY: &str = "a bearer key that never prints, 32+ bytes";

/// A head's `Debug`, compact or pretty, and that of the parse holding it, say a key is presented and never hold it.
#[test]
fn test_a_head_never_prints_its_key() -> TestResult {
    let request = format!("GET /v1/targets HTTP/1.1\r\nAuthorization: Bearer {KEY}\r\n\r\n");
    let parsed = parse_head(
        request.as_bytes(),
        &HttpLimits::new(16, Duration::from_secs(1)),
    );
    let ParsedHead::Complete(head) = &parsed else {
        return Err(TestError::new(format!("a whole request: {parsed:?}")));
    };
    let printed = format!("{head:?}");
    for shown in [&printed, &format!("{head:#?}"), &format!("{parsed:?}")] {
        hides(shown, KEY.as_bytes())?;
    }
    assert!(printed.contains("bearer: Some(<redacted>)"), "{printed}");
    assert!(printed.contains("target: \"/v1/targets\""), "{printed}");
    Ok(())
}
