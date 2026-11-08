/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::{TestError, TestResult};
use ultragraphdb_transport_http_client::{HttpReply, ParsedReply, parse_reply};

fn whole(bytes: &[u8]) -> TestResult<HttpReply> {
    if let ParsedReply::Complete(reply) = parse_reply(bytes, 8) {
        return Ok(reply);
    }
    Err(TestError::new("a whole reply"))
}

/// A reply reads to its status, headers and body, and bytes after the body change nothing.
#[test]
fn test_a_reply_reads_whole() -> TestResult {
    let reply = whole(b"HTTP/1.1 200 OK\r\nX-Revision: 7\r\ncontent-length: 2\r\n\r\nokTRAILING")?;
    assert_eq!(reply.status(), 200);
    assert_eq!(reply.header("x-revision"), Some("7"));
    assert_eq!(reply.header("absent"), None);
    assert_eq!(reply.body(), b"ok");
    assert_eq!(reply.into_body(), b"ok");
    Ok(())
}

/// A reply is not whole until its head and its body are.
#[test]
fn test_a_reply_waits_for_its_bytes() {
    for partial in [
        &b""[..],
        b"HTTP/1.1 200 OK\r\n",
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\no",
    ] {
        assert_eq!(parse_reply(partial, 8), ParsedReply::Incomplete);
    }
}

/// What is no reply is refused, whatever its length: a peer may not fill this side's memory.
#[test]
fn test_what_is_no_reply_is_refused() {
    let endless = vec![b'a'; 16 * 1024];
    for wrong in [
        &b"HTTP/2 200 OK\r\nContent-Length: 0\r\n\r\n"[..],
        b"HTTP/1.1 ok\r\nContent-Length: 0\r\n\r\n",
        b"HTTP/1.1 99\r\nContent-Length: 0\r\n\r\n",
        b"HTTP/1.1 1000\r\nContent-Length: 0\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nno colon\r\n\r\n",
        b"HTTP/1.1 200 OK\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nContent-Length: 1\r\n\r\nx",
        b"HTTP/1.1 200 OK\r\nContent-Length: -1\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\n",
        b"\xff\r\n\r\n",
        &endless,
    ] {
        assert!(
            matches!(parse_reply(wrong, 8), ParsedReply::Malformed(_)),
            "{}",
            String::from_utf8_lossy(wrong)
        );
    }
}

/// A head of 8 KiB, half the 16 KiB a status line and its headers take, is read whole.
#[test]
fn test_a_long_head_within_its_bound_is_read() -> TestResult {
    let header = "a".repeat(8 * 1024);
    let reply = whole(
        format!("HTTP/1.1 200 OK\r\nX-Long: {header}\r\nContent-Length: 0\r\n\r\n").as_bytes(),
    )?;
    assert_eq!(reply.header("x-long"), Some(header.as_str()));
    Ok(())
}
