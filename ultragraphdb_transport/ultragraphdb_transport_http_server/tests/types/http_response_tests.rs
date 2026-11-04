/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_transport_http_server::HttpResponse;

/// A response states its status and reason, its headers, its body's length and that it closes.
#[test]
fn test_a_head_states_what_a_client_reads() {
    let response = HttpResponse::new(200)
        .with_header("X-Revision", "7")
        .with_body("settings");
    assert_eq!(
        response.head(),
        "HTTP/1.1 200 OK\r\nX-Revision: 7\r\nContent-Length: 8\r\nConnection: close\r\n\r\n"
    );
    assert_eq!(response.body(), b"settings");
    assert_eq!(response.status(), 200);
}

/// Each status the control plane answers with has its reason.
#[test]
fn test_each_status_has_its_reason() {
    for (status, reason) in [
        (201, "Created"),
        (202, "Accepted"),
        (400, "Bad Request"),
        (401, "Unauthorized"),
        (403, "Forbidden"),
        (404, "Not Found"),
        (405, "Method Not Allowed"),
        (408, "Request Timeout"),
        (409, "Conflict"),
        (413, "Content Too Large"),
        (422, "Unprocessable Content"),
        (431, "Request Header Fields Too Large"),
        (501, "Not Implemented"),
        (503, "Service Unavailable"),
    ] {
        let line = format!("HTTP/1.1 {status} {reason}\r\n");
        assert!(
            HttpResponse::new(status).head().starts_with(&line),
            "{status}"
        );
    }
}
