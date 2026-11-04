/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use ultragraphdb_tool_bdd::{TestError, TestResult};
use ultragraphdb_transport_http_server::{HttpHead, HttpLimits, ParsedHead, parse_head};

fn limits() -> HttpLimits {
    HttpLimits::new(16, Duration::from_secs(1))
}

fn complete(bytes: &[u8]) -> TestResult<HttpHead> {
    if let ParsedHead::Complete(head) = parse_head(bytes, &limits()) {
        return Ok(head);
    }
    Err(TestError::new("a whole request"))
}

/// A request reads to its method, target, key and body.
#[test]
fn test_a_request_reads_whole() -> TestResult {
    let head = complete(
        b"POST /v1/targets/ci/revisions?base=3 HTTP/1.1\r\nauthorization: Bearer k\r\nContent-Length: 4\r\n\r\nbody",
    )?;
    assert_eq!(head.method(), "POST");
    assert_eq!(head.target(), "/v1/targets/ci/revisions?base=3");
    assert_eq!(head.bearer(), Some("k"));
    assert_eq!(head.body_len(), 4);
    let bare = complete(b"GET /health HTTP/1.0\r\n\r\n")?;
    assert_eq!((bare.bearer(), bare.body_len()), (None, 0));
    Ok(())
}

/// A request is not whole until its head and its body are, and bytes after the body change nothing.
#[test]
fn test_a_request_waits_for_its_bytes() -> TestResult {
    for partial in [
        &b""[..],
        b"GET /health HTTP/1.1\r\n",
        b"POST / HTTP/1.1\r\nContent-Length: 4\r\n\r\nbod",
    ] {
        assert_eq!(parse_head(partial, &limits()), ParsedHead::Incomplete);
    }
    let head = complete(b"POST / HTTP/1.1\r\nContent-Length: 1\r\n\r\nxTRAILING")?;
    assert_eq!(head.body_len(), 1);
    Ok(())
}

/// Each malformed or unservable request gets its own status, read no further.
#[test]
fn test_a_request_is_refused_by_its_status() {
    let long_head = [
        b"GET /".as_slice(),
        &[b'a'; HttpLimits::HEAD_MAX],
        b" HTTP/1.1\r\n\r\n",
    ]
    .concat();
    let no_end = vec![b'a'; HttpLimits::HEAD_MAX];
    let cases: [(&[u8], u16); 11] = [
        (b"GET\r\n\r\n", 400),
        (b"GET / HTTP/1.1 extra\r\n\r\n", 400),
        (b"get / HTTP/1.1\r\n\r\n", 400),
        (b"GET health HTTP/1.1\r\n\r\n", 400),
        (b"GET / HTTP/2\r\n\r\n", 400),
        (b"GET / HTTP/1.1\r\nno colon\r\n\r\n", 400),
        (
            b"POST / HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 1\r\n\r\nx",
            400,
        ),
        (b"POST / HTTP/1.1\r\nContent-Length: -1\r\n\r\n", 400),
        (b"POST / HTTP/1.1\r\nContent-Length: 17\r\n\r\n", 413),
        (
            b"POST / HTTP/1.1\r\nContent-Length: 99999999999999999999999\r\n\r\n",
            413,
        ),
        (
            b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n",
            501,
        ),
    ];
    for (bytes, status) in cases {
        assert_eq!(
            parse_head(bytes, &limits()),
            ParsedHead::Refused(status),
            "{}",
            String::from_utf8_lossy(bytes)
        );
    }
    assert_eq!(parse_head(&long_head, &limits()), ParsedHead::Refused(431));
    assert_eq!(parse_head(&no_end, &limits()), ParsedHead::Refused(431));
    assert_eq!(
        parse_head(b"GET /\xff HTTP/1.1\r\n\r\n", &limits()),
        ParsedHead::Refused(400)
    );
}

/// A key is the bearer's alone; another scheme presents none.
#[test]
fn test_only_a_bearer_presents_a_key() -> TestResult {
    let basic = complete(b"GET / HTTP/1.1\r\nAuthorization: Basic abc\r\n\r\n")?;
    assert_eq!(basic.bearer(), None);
    Ok(())
}
