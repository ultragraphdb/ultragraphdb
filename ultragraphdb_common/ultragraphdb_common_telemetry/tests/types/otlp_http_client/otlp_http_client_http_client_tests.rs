/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::net::TcpListener;
use std::thread;
use std::time::{Duration, Instant};

use futures::executor::block_on;
use opentelemetry_http::{Bytes, HttpClient, HttpError, Request, Response};
use ultragraphdb_common_telemetry::OtlpHttpClient;
use ultragraphdb_transport_tls::{TestPki, TlsConnector, TlsTrust};

use crate::test_support::{serve_once, serve_once_tls};

const TIMEOUT: Duration = Duration::from_secs(5);

/// A POST of `body` to `url` with the exporter's content type and one header of the caller's.
fn post(url: &str, body: &'static [u8]) -> Request<Bytes> {
    Request::builder()
        .method("POST")
        .uri(url)
        .header("content-type", "application/x-protobuf")
        .header("x-tenant", "acme")
        .body(Bytes::from_static(body))
        .expect("a request")
}

fn send(request: Request<Bytes>) -> Result<Response<Bytes>, HttpError> {
    block_on(OtlpHttpClient::new(TIMEOUT).send_bytes(request))
}

/// The response `server_response` gives a POST to `/v1/traces`.
fn answered(server_response: &'static [u8]) -> Result<Response<Bytes>, HttpError> {
    let (url, server) = serve_once(server_response);
    let result = send(post(&format!("{url}/v1/traces"), b"payload"));
    server.join().expect("the server thread");
    result
}

#[test]
fn test_the_request_carries_its_framing_the_callers_headers_and_its_body() {
    let (url, server) = serve_once(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
    let mut request = post(&format!("{url}/v1/traces?q=1"), b"payload");
    // The caller's own framing headers are dropped for the client's.
    request
        .headers_mut()
        .insert("content-length", "999".parse().expect("a value"));
    request
        .headers_mut()
        .insert("host", "elsewhere".parse().expect("a value"));
    send(request).expect("a response");
    let sent = String::from_utf8(server.join().expect("the server thread")).expect("text");
    let authority = url.trim_start_matches("http://");
    assert_eq!(
        sent,
        format!(
            "POST /v1/traces?q=1 HTTP/1.1\r\nHost: {authority}\r\nContent-Length: 7\r\nConnection: close\r\n\
             content-type: application/x-protobuf\r\nx-tenant: acme\r\n\r\npayload"
        )
    );
}

#[test]
fn test_a_content_length_response_gives_its_status_headers_and_body() {
    let response = answered(
        b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Length: 3\r\n\r\nabcIGNORED",
    )
    .expect("a response");
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get("content-type").map(|v| v.as_bytes()),
        Some(&b"application/x-protobuf"[..])
    );
    assert_eq!(response.body().as_ref(), b"abc");
}

#[test]
fn test_a_chunked_response_is_decoded_and_its_trailers_ignored() {
    let response = answered(
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4;ext=1\r\nspan\r\n6\r\n-data!\r\n0\r\nX-Trailer: t\r\n\r\n",
    )
    .expect("a response");
    assert_eq!(response.body().as_ref(), b"span-data!");
}

#[test]
fn test_a_response_without_framing_is_read_to_the_close() {
    let response = answered(b"HTTP/1.0 200 OK\r\n\r\nuntil close").expect("a response");
    assert_eq!(response.body().as_ref(), b"until close");
}

#[test]
fn test_an_error_status_is_a_response_not_an_error() {
    let response = answered(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 4\r\n\r\nbusy")
        .expect("a response");
    assert_eq!(response.status(), 503);
    assert_eq!(response.body().as_ref(), b"busy");
}

#[test]
fn test_malformed_responses_are_errors() {
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
    ] {
        let error = answered(raw).expect_err(reason);
        assert_eq!(error.to_string(), reason);
    }
}

/// The client verifying `https://` against `pki`'s authority.
fn tls_client(pki: &TestPki) -> OtlpHttpClient {
    let trust = TlsTrust::from_pem("test CA", pki.ca_pem().as_bytes()).expect("the CA");
    OtlpHttpClient::new(TIMEOUT).with_tls(TlsConnector::new(trust))
}

#[test]
fn test_an_https_request_is_sent_and_answered_over_verified_tls() {
    let pki = TestPki::new();
    let (url, server) = serve_once_tls(&pki, b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
    let client = tls_client(&pki);
    assert!(client.tls().is_some());
    let response = block_on(client.send_bytes(post(&format!("{url}/v1/traces"), b"payload")))
        .expect("a response over TLS");
    assert_eq!(response.status(), 200);
    assert_eq!(response.body().as_ref(), b"ok");
    let sent = String::from_utf8(server.join().expect("the server thread")).expect("text");
    let authority = url.trim_start_matches("https://");
    assert!(
        sent.starts_with(&format!(
            "POST /v1/traces HTTP/1.1\r\nHost: {authority}\r\n"
        )),
        "{sent}"
    );
    assert!(sent.ends_with("\r\n\r\npayload"), "{sent}");
}

#[test]
fn test_an_https_request_to_an_untrusted_server_is_refused() {
    let (pki, other) = (TestPki::new(), TestPki::new());
    let (url, server) = serve_once_tls(&pki, b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
    let error = block_on(tls_client(&other).send_bytes(post(&format!("{url}/v1/traces"), b"x")))
        .expect_err("another CA's certificate");
    // rustls's text for a chain to no trusted anchor, behind the connector's handshake error.
    assert!(
        error
            .to_string()
            .starts_with("TLS handshake with \"localhost\" failed:")
            && error.to_string().contains("UnknownIssuer"),
        "{error}"
    );
    // The server's side of the refused handshake ends with or without an error of its own; only the client's matters.
    drop(server.join());
}

#[test]
fn test_https_without_a_connector_is_refused_before_connecting() {
    let client = OtlpHttpClient::new(TIMEOUT);
    assert!(client.tls().is_none());
    let error = block_on(client.send_bytes(post("https://127.0.0.1:4318/v1/traces", b"x")))
        .expect_err("no connector");
    assert_eq!(
        error.to_string(),
        "https needs a TLS connector: https://127.0.0.1:4318/v1/traces"
    );
}

#[test]
fn test_a_scheme_other_than_http_or_https_is_refused() {
    let error = send(post("ftp://127.0.0.1:4318/v1/traces", b"x")).expect_err("not HTTP");
    assert_eq!(
        error.to_string(),
        "http:// or https:// only: ftp://127.0.0.1:4318/v1/traces"
    );
}

#[test]
fn test_a_refused_connection_is_an_error() {
    let port = TcpListener::bind("127.0.0.1:0")
        .expect("a loopback port")
        .local_addr()
        .expect("its address")
        .port();
    // The listener is dropped: nothing accepts on the port.
    let error =
        send(post(&format!("http://127.0.0.1:{port}/v1/traces"), b"x")).expect_err("refused");
    assert!(
        error
            .to_string()
            .starts_with(&format!("127.0.0.1:{port}: ")),
        "{error}"
    );
}

#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "the test serves on a thread of its own and times the client by the wall clock"
)]
fn test_a_silent_server_times_out_within_the_clients_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let url = format!(
        "http://{}/v1/traces",
        listener.local_addr().expect("its address")
    );
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("one connection");
        thread::sleep(Duration::from_millis(600));
        drop(stream);
    });
    let started = Instant::now();
    let result =
        block_on(OtlpHttpClient::new(Duration::from_millis(200)).send_bytes(post(&url, b"x")));
    assert!(result.is_err(), "{result:?}");
    assert!(
        started.elapsed() < Duration::from_millis(550),
        "{:?}",
        started.elapsed()
    );
    server.join().expect("the server thread");
}
