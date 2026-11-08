/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The probe asked of the control plane's own HTTP server, which a role's `/health` and `/ready` are served by.

use std::cell::Cell;
use std::time::Duration;

use compio::net::TcpListener;
use compio::runtime::{spawn, spawn_blocking};
use ultragraphdb_tests::{http_body, http_status};
use ultragraphdb_tool_bdd::{TestError, TestResult};
use ultragraphdb_transport_http_server::{
    HttpLimits, HttpRequest, HttpResponse, HttpService, serve_http,
};

/// What the server answers at a path it serves.
pub(crate) const SERVED: &str = "served";

/// What it answers at any other path, with 503.
pub(crate) const NOT_YET: &str = "not yet";

/// A server answering 200 at `/health` and 503 elsewhere, as a role answers before it is ready.
#[derive(Debug, Clone, Copy)]
struct Starting;

impl HttpService for Starting {
    async fn call(&self, request: HttpRequest) -> HttpResponse {
        if request.path() == "/health" {
            HttpResponse::new(200).with_body(SERVED)
        } else {
            HttpResponse::new(503).with_body(NOT_YET)
        }
    }
}

/// Serves [`Starting`] at a fresh port on this runtime, its address returned.
pub(crate) async fn serving() -> TestResult<String> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?.to_string();
    spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let limits = HttpLimits::new(0, Duration::from_secs(5));
            serve_http(stream, &limits, &Starting).await;
        }
    })
    .detach();
    Ok(address)
}

/// A server answering 503 at every path for its first `after` requests, and 200 from then on: a role that becomes ready later
/// than its start.
#[derive(Debug)]
struct ReadyAfter {
    asked: Cell<u32>,
    after: u32,
}

impl HttpService for ReadyAfter {
    async fn call(&self, _request: HttpRequest) -> HttpResponse {
        let asked = self.asked.get();
        self.asked.set(asked.saturating_add(1));
        if asked < self.after {
            HttpResponse::new(503).with_body(NOT_YET)
        } else {
            HttpResponse::new(200).with_body(SERVED)
        }
    }
}

/// Serves [`ReadyAfter`] of `after` requests at a fresh port on this runtime, its address returned.
pub(crate) async fn serving_ready_after(after: u32) -> TestResult<String> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?.to_string();
    spawn(async move {
        let service = ReadyAfter {
            asked: Cell::new(0),
            after,
        };
        while let Ok((stream, _)) = listener.accept().await {
            let limits = HttpLimits::new(0, Duration::from_secs(5));
            serve_http(stream, &limits, &service).await;
        }
    })
    .detach();
    Ok(address)
}

/// What `ask` returns, asked off the runtime serving it, since the probe blocks.
pub(crate) async fn off_runtime<T: Send + 'static>(
    ask: impl FnOnce() -> TestResult<T> + Send + 'static,
) -> TestResult<T> {
    spawn_blocking(ask)
        .await
        .map_err(|_| TestError::new("the probe's thread ended without an answer"))?
}

#[test]
fn test_the_status_is_the_answers() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let address = serving().await?;
        let (healthy, ready) = off_runtime(move || {
            Ok((
                http_status(&address, "/health")?,
                http_status(&address, "/ready")?,
            ))
        })
        .await?;
        assert_eq!((healthy, ready), (200, 503));
        Ok(())
    })
}

#[test]
fn test_the_body_is_what_follows_the_head() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let address = serving().await?;
        let (healthy, ready) = off_runtime(move || {
            Ok((
                http_body(&address, "/health")?,
                http_body(&address, "/ready")?,
            ))
        })
        .await?;
        assert_eq!((healthy.as_str(), ready.as_str()), (SERVED, NOT_YET));
        Ok(())
    })
}

#[test]
fn test_nothing_listening_is_an_error() -> TestResult {
    let free = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .to_string();
    assert!(http_status(&free, "/health").is_err());
    assert!(http_body(&free, "/health").is_err());
    Ok(())
}
