/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::BufResult;
use compio::io::{AsyncReadExt, AsyncWriteExt};
use compio::net::{TcpListener, TcpStream};
use compio::runtime::{Runtime, spawn};
use std::time::Duration;
use ultragraphdb_common_service::{HealthService, Readiness};
use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_transport_http_server::{HttpLimits, serve_http};

/// What the service answers `request` with.
async fn asked(service: &HealthService, request: &str) -> TestResult<String> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let held = service.clone();
    spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            let limits = HttpLimits::new(0, Duration::from_secs(5));
            serve_http(stream, &limits, &held).await;
        }
    })
    .detach();
    let mut stream = TcpStream::connect(address).await?;
    let BufResult(written, _) = stream.write_all(request.as_bytes().to_vec()).await;
    written?;
    let BufResult(read, bytes) = stream.read_to_end(Vec::new()).await;
    let _count: usize = read?;
    Ok(String::from_utf8(bytes)?)
}

/// `/health` answers at once, `/ready` only when ready, anything else by its status, and only GET is served.
#[test]
fn test_health_answers_at_once_and_ready_only_when_ready() -> TestResult {
    Runtime::new()?.block_on(async {
        let readiness = Readiness::default();
        let service = HealthService::new(readiness.clone());
        let status = |text: String| text.split(' ').nth(1).map(str::to_string);
        let health = asked(&service, "GET /health HTTP/1.1\r\n\r\n").await?;
        assert_eq!(status(health).as_deref(), Some("200"));
        let waiting = asked(&service, "GET /ready HTTP/1.1\r\n\r\n").await?;
        assert_eq!(status(waiting).as_deref(), Some("503"));
        readiness.set(true);
        let ready = asked(&service, "GET /ready HTTP/1.1\r\n\r\n").await?;
        assert_eq!(status(ready).as_deref(), Some("200"));
        let other = asked(&service, "GET /other HTTP/1.1\r\n\r\n").await?;
        assert_eq!(status(other).as_deref(), Some("404"));
        let method = asked(&service, "POST /health HTTP/1.1\r\n\r\n").await?;
        assert!(method.contains("Allow: GET"), "{method}");
        assert_eq!(status(method).as_deref(), Some("405"));
        Ok(())
    })
}
