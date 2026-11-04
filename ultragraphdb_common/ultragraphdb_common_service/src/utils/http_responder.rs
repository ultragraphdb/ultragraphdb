/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::health_request::HealthRequest;

/// The largest request the listener reads, past which it answers 400: `/health` and `/ready` take nothing.
pub(crate) const REQUEST_MAX: usize = 4096;

const END_OF_HEADERS: &[u8] = b"\r\n\r\n";

/// What `bytes`, all that has arrived on a connection, ask. The request line alone decides it, and headers are skipped.
#[must_use]
pub fn parse_request(bytes: &[u8]) -> HealthRequest {
    let end = bytes
        .windows(END_OF_HEADERS.len())
        .position(|w| w == END_OF_HEADERS);
    let Some(end) = end else {
        return if bytes.len() >= REQUEST_MAX {
            HealthRequest::BadRequest
        } else {
            HealthRequest::Incomplete
        };
    };
    let Some(head) = bytes
        .get(..end)
        .and_then(|head| std::str::from_utf8(head).ok())
    else {
        return HealthRequest::BadRequest;
    };
    let line = head.split("\r\n").next().unwrap_or_default();
    let mut parts = line.split(' ');
    let (Some(method), Some(path), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return HealthRequest::BadRequest;
    };
    if !version.starts_with("HTTP/1.") || !path.starts_with('/') || method.is_empty() {
        return HealthRequest::BadRequest;
    }
    match (method, path) {
        ("GET", "/health") => HealthRequest::Health,
        ("GET", "/ready") => HealthRequest::Ready,
        ("GET", _) => HealthRequest::NotFound,
        _ => HealthRequest::MethodNotAllowed,
    }
}

/// The whole response to `request`, given whether the role is ready. `/health` answers once the process runs; `/ready`
/// answers 200 only once the role is ready. Every response closes the connection.
#[must_use]
pub fn response(request: HealthRequest, ready: bool) -> &'static str {
    match request {
        HealthRequest::Health => OK,
        HealthRequest::Ready if ready => OK,
        HealthRequest::Ready => NOT_READY,
        HealthRequest::NotFound => NOT_FOUND,
        HealthRequest::MethodNotAllowed => NOT_ALLOWED,
        HealthRequest::Incomplete | HealthRequest::BadRequest => BAD_REQUEST,
    }
}

const OK: &str = "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nok\n";
const NOT_READY: &str = "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 10\r\nConnection: close\r\n\r\nnot ready\n";
const NOT_FOUND: &str = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
const NOT_ALLOWED: &str = "HTTP/1.1 405 Method Not Allowed\r\nAllow: GET\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
const BAD_REQUEST: &str =
    "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
