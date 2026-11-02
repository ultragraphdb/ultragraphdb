/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use opentelemetry_http::{Bytes, HttpError, Response};
use ultragraphdb_common_telemetry::decode_response;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

/// The error `result` holds, or a [`TestError`] naming what was `expected` to fail.
fn refused(result: Result<Response<Bytes>, HttpError>, expected: &str) -> TestResult<HttpError> {
    let Err(error) = result else {
        return Err(TestError::new(format!("{expected}: decoded")));
    };
    Ok(error)
}

#[test]
fn test_a_content_length_response_gives_its_status_headers_and_body() -> TestResult {
    let response = decode_response(
        b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Length: 3\r\n\r\nabcIGNORED",
    )
    .context("a response")?;
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get("content-type").map(|v| v.as_bytes()),
        Some(&b"application/x-protobuf"[..])
    );
    assert_eq!(response.body().as_ref(), b"abc");
    Ok(())
}

#[test]
fn test_a_zero_content_length_gives_an_empty_body() -> TestResult {
    let response = decode_response(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\ntrailing")
        .context("a response")?;
    assert_eq!(response.body().as_ref(), b"");
    Ok(())
}

#[test]
fn test_a_chunked_response_is_decoded_and_its_trailers_ignored() -> TestResult {
    let response = decode_response(
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4;ext=1\r\nspan\r\n6\r\n-data!\r\n0\r\nX-Trailer: t\r\n\r\n",
    )
    .context("a response")?;
    assert_eq!(response.body().as_ref(), b"span-data!");
    Ok(())
}

#[test]
fn test_chunked_framing_takes_precedence_over_content_length_and_names_ignore_case() -> TestResult {
    let response = decode_response(
        b"HTTP/1.1 200 OK\r\ncontent-length: 1\r\nTRANSFER-ENCODING: gzip, Chunked\r\n\r\n3\r\nabc\r\n0\r\n\r\n",
    )
    .context("a response")?;
    assert_eq!(response.body().as_ref(), b"abc");
    Ok(())
}

#[test]
fn test_a_response_without_framing_is_read_to_the_close() -> TestResult {
    let response = decode_response(b"HTTP/1.0 200 OK\r\n\r\nuntil close").context("a response")?;
    assert_eq!(response.body().as_ref(), b"until close");
    Ok(())
}

#[test]
fn test_an_error_status_is_a_response_not_an_error() -> TestResult {
    let response =
        decode_response(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 4\r\n\r\nbusy")
            .context("a response")?;
    assert_eq!(response.status(), 503);
    assert_eq!(response.body().as_ref(), b"busy");
    Ok(())
}

#[test]
fn test_malformed_responses_are_errors() -> TestResult {
    for (raw, reason) in [
        (
            &b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nshort"[..],
            "body of 5 bytes, 9 announced",
        ),
        (
            b"HTTP/1.1 OK\r\n\r\n",
            "malformed status line \"HTTP/1.1 OK\"",
        ),
        (
            b"HTTP/1.1 200 OK\r\nno colon\r\n\r\n",
            "malformed header \"no colon\"",
        ),
        (
            b"HTTP/1.1 200 OK\r\nContent-Length: 0",
            "response without a complete head",
        ),
        (b"", "response without a complete head"),
        (
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n",
            "malformed chunk size \"zz\"",
        ),
        (
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n9\r\nshort\r\n",
            "chunk shorter than its size",
        ),
        (
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nabXX0\r\n\r\n",
            "chunk without its line end",
        ),
        (
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nab",
            "chunk without its line end",
        ),
        (
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3",
            "chunk size without its line end",
        ),
    ] {
        let error = refused(decode_response(raw), reason)?;
        assert_eq!(error.to_string(), reason, "{raw:?}");
    }
    Ok(())
}

#[test]
fn test_a_content_length_that_is_no_number_is_an_error() -> TestResult {
    // Rust's own text for a decimal that does not parse (`ParseIntError`).
    let error = refused(
        decode_response(b"HTTP/1.1 200 OK\r\nContent-Length: ten\r\n\r\n"),
        "no number",
    )?;
    assert_eq!(error.to_string(), "invalid digit found in string");
    Ok(())
}

#[test]
fn test_a_head_that_is_no_utf_8_is_an_error() -> TestResult {
    let error = refused(decode_response(b"HTTP/1.1 200 \xff\r\n\r\n"), "not UTF-8")?;
    assert!(error.to_string().starts_with("invalid utf-8"), "{error}");
    Ok(())
}
