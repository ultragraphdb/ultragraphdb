/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_service::{HealthRequest, parse_request, response};

fn status(request: HealthRequest, ready: bool) -> Option<&'static str> {
    response(request, ready).split(' ').nth(1)
}

/// `Content-Length` must count the body's bytes, or a client waits for what never comes.
#[test]
fn test_every_response_states_its_body_length() {
    let requests = [
        HealthRequest::Health,
        HealthRequest::Ready,
        HealthRequest::NotFound,
        HealthRequest::MethodNotAllowed,
        HealthRequest::BadRequest,
        HealthRequest::Incomplete,
    ];
    for request in requests {
        for ready in [true, false] {
            let text = response(request, ready);
            let (head, body) = text.split_once("\r\n\r\n").unwrap_or((text, ""));
            let stated = format!("Content-Length: {}", body.len());
            assert!(head.contains(&stated), "{text}");
            assert!(head.contains("Connection: close"), "{text}");
        }
    }
}

#[test]
fn test_health_answers_at_once_and_ready_only_when_ready() {
    assert_eq!(status(HealthRequest::Health, false), Some("200"));
    assert_eq!(status(HealthRequest::Ready, true), Some("200"));
    assert_eq!(status(HealthRequest::Ready, false), Some("503"));
    assert_eq!(status(HealthRequest::NotFound, true), Some("404"));
    assert_eq!(status(HealthRequest::MethodNotAllowed, true), Some("405"));
    assert_eq!(status(HealthRequest::BadRequest, true), Some("400"));
}

#[test]
fn test_a_request_is_read_by_its_request_line() {
    let cases: [(&[u8], HealthRequest); 10] = [
        (
            b"GET /health HTTP/1.1\r\nHost: x\r\n\r\n",
            HealthRequest::Health,
        ),
        (b"GET /ready HTTP/1.0\r\n\r\n", HealthRequest::Ready),
        (b"GET /other HTTP/1.1\r\n\r\n", HealthRequest::NotFound),
        (
            b"POST /health HTTP/1.1\r\n\r\n",
            HealthRequest::MethodNotAllowed,
        ),
        (b"GET /health", HealthRequest::Incomplete),
        (b"", HealthRequest::Incomplete),
        (b"GET /health HTTP/2\r\n\r\n", HealthRequest::BadRequest),
        (b"GET health HTTP/1.1\r\n\r\n", HealthRequest::BadRequest),
        (b"GET  /health HTTP/1.1\r\n\r\n", HealthRequest::BadRequest),
        (b"\xff /health HTTP/1.1\r\n\r\n", HealthRequest::BadRequest),
    ];
    for (bytes, expected) in cases {
        assert_eq!(
            parse_request(bytes),
            expected,
            "{}",
            String::from_utf8_lossy(bytes)
        );
    }
}

/// A request is bounded: one byte short of the bound may still end; at the bound it no longer may.
#[test]
fn test_a_request_past_its_bound_is_refused() {
    assert_eq!(parse_request(&[b'a'; 4095]), HealthRequest::Incomplete);
    assert_eq!(parse_request(&[b'a'; 4096]), HealthRequest::BadRequest);
}
