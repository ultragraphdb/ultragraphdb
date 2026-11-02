/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use futures::executor::block_on;
use opentelemetry_http::{Bytes, HttpClient, HttpError, Request, Response};
use ultragraphdb_common_telemetry::{Dial, OtlpHttpClient};
use ultragraphdb_transport_tls::{TestPki, TlsConnector, TlsTrust};

use crate::collector::{CollectorDial, TlsCollectorDial};

const TIMEOUT: Duration = Duration::from_secs(5);
const OK_EMPTY: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";

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

fn send<D: Dial + 'static>(
    client: &OtlpHttpClient<D>,
    request: Request<Bytes>,
) -> Result<Response<Bytes>, HttpError> {
    block_on(client.send_bytes(request))
}

/// The client verifying `https://` against `pki`'s authority, over `dial`.
fn tls_client<D: Dial>(dial: D, pki: &TestPki) -> OtlpHttpClient<D> {
    let trust = TlsTrust::from_pem("test CA", pki.ca_pem().as_bytes()).expect("the CA");
    OtlpHttpClient::over(dial, TIMEOUT).with_tls(TlsConnector::new(trust))
}

#[test]
fn test_the_request_carries_its_framing_the_callers_headers_and_its_body() {
    let dial = CollectorDial::answering(OK_EMPTY);
    let client = OtlpHttpClient::over(dial.clone(), TIMEOUT);
    let mut request = post("http://collector:4318/v1/traces?q=1", b"payload");
    // The caller's own framing headers are dropped for the client's.
    request
        .headers_mut()
        .insert("content-length", "999".parse().expect("a value"));
    request
        .headers_mut()
        .insert("host", "elsewhere".parse().expect("a value"));
    let response = send(&client, request).expect("a response");
    assert_eq!(response.status(), 200);
    assert_eq!(
        dial.received(),
        "POST /v1/traces?q=1 HTTP/1.1\r\nHost: collector:4318\r\nContent-Length: 7\r\nConnection: close\r\n\
         content-type: application/x-protobuf\r\nx-tenant: acme\r\n\r\npayload"
    );
    assert_eq!(dial.dialed(), [("collector".to_string(), 4318, TIMEOUT)]);
}

#[test]
fn test_a_scheme_without_a_port_dials_its_default_and_leaves_the_port_out_of_host() {
    let dial = CollectorDial::answering(OK_EMPTY);
    send(
        &OtlpHttpClient::over(dial.clone(), TIMEOUT),
        post("http://collector/v1/traces", b"x"),
    )
    .expect("a response");
    assert_eq!(dial.dialed(), [("collector".to_string(), 80, TIMEOUT)]);
    assert!(
        dial.received().contains("\r\nHost: collector\r\n"),
        "{}",
        dial.received()
    );

    let pki = TestPki::new();
    let tls = TlsCollectorDial::answering(&pki, OK_EMPTY);
    send(
        &tls_client(tls.clone(), &pki),
        post("https://localhost/v1/traces", b"x"),
    )
    .expect("a response over TLS");
    assert_eq!(
        tls.plain().dialed(),
        [("localhost".to_string(), 443, TIMEOUT)]
    );
    assert!(tls.plain().received().contains("\r\nHost: localhost\r\n"));
}

#[test]
fn test_an_ipv6_host_is_dialed_without_its_brackets() {
    let dial = CollectorDial::answering(OK_EMPTY);
    send(
        &OtlpHttpClient::over(dial.clone(), TIMEOUT),
        post("http://[::1]:4318/v1/traces", b"x"),
    )
    .expect("a response");
    assert_eq!(dial.dialed(), [("::1".to_string(), 4318, TIMEOUT)]);
    assert!(
        dial.received().contains("\r\nHost: [::1]:4318\r\n"),
        "{}",
        dial.received()
    );
}

#[test]
fn test_the_response_is_decoded_from_what_the_collector_answers() {
    let dial = CollectorDial::answering(b"HTTP/1.1 202 Accepted\r\nContent-Length: 2\r\n\r\nok");
    let response = send(
        &OtlpHttpClient::over(dial, TIMEOUT),
        post("http://collector/v1/traces", b"x"),
    )
    .expect("a response");
    assert_eq!(response.status(), 202);
    assert_eq!(response.body().as_ref(), b"ok");
}

/// A response of `total` bytes in all, head included: 200 with a body of the rest.
fn response_of(total: usize) -> Vec<u8> {
    let head_for = |body: usize| format!("HTTP/1.1 200 OK\r\nContent-Length: {body}\r\n\r\n");
    // The head's length depends on the body's digits; the body is what remains once the head is written.
    let body = total - head_for(total).len();
    let mut response = head_for(body).into_bytes();
    response.resize(total, b'x');
    assert_eq!(response.len(), total);
    response
}

#[test]
fn test_a_response_of_4_mib_is_read_and_one_byte_more_is_refused() {
    // 4 MiB, the bound opentelemetry-http's own clients put on a response (`MAX_RESPONSE_BODY_BYTES`).
    const BOUND: usize = 4 * 1024 * 1024;
    let at_bound = CollectorDial::answering(response_of(BOUND));
    let response = send(
        &OtlpHttpClient::over(at_bound, TIMEOUT),
        post("http://collector/v1/traces", b"x"),
    )
    .expect("a response at the bound");
    let announced = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok());
    assert_eq!(Some(response.body().len()), announced);
    let past_bound = CollectorDial::answering(response_of(BOUND + 1));
    let error = send(
        &OtlpHttpClient::over(past_bound, TIMEOUT),
        post("http://collector/v1/traces", b"x"),
    )
    .expect_err("a response past the bound");
    assert_eq!(error.to_string(), "response over 4194304 bytes");
}

#[test]
fn test_the_request_is_flushed_to_a_connection_that_sends_only_what_is_flushed() {
    let dial = CollectorDial::buffering(OK_EMPTY);
    let response = send(
        &OtlpHttpClient::over(dial.clone(), TIMEOUT),
        post("http://collector/v1/traces", b"x"),
    )
    .expect("a response once the request is flushed");
    assert_eq!(response.status(), 200);
    assert!(
        dial.received().ends_with("\r\n\r\nx"),
        "{}",
        dial.received()
    );
}

#[test]
fn test_an_https_request_is_sent_and_answered_over_verified_tls() {
    let pki = TestPki::new();
    let tls = TlsCollectorDial::answering(&pki, b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
    let client = tls_client(tls.clone(), &pki);
    assert!(client.tls().is_some());
    let response = send(
        &client,
        post("https://localhost:4318/v1/traces", b"payload"),
    )
    .expect("a response over TLS");
    assert_eq!(response.status(), 200);
    assert_eq!(response.body().as_ref(), b"ok");
    let sent = tls.plain().received();
    assert!(
        sent.starts_with("POST /v1/traces HTTP/1.1\r\nHost: localhost:4318\r\n"),
        "{sent}"
    );
    assert!(sent.ends_with("\r\n\r\npayload"), "{sent}");
}

#[test]
fn test_an_https_request_to_an_untrusted_server_is_refused_before_the_request() {
    let (pki, other) = (TestPki::new(), TestPki::new());
    let tls = TlsCollectorDial::answering(&pki, OK_EMPTY);
    let error = send(
        &tls_client(tls.clone(), &other),
        post("https://localhost/v1/traces", b"x"),
    )
    .expect_err("another CA's certificate");
    // rustls's text for a chain to no trusted anchor, behind the connector's handshake error.
    assert!(
        error
            .to_string()
            .starts_with("TLS handshake with \"localhost\" failed:")
            && error.to_string().contains("UnknownIssuer"),
        "{error}"
    );
    assert_eq!(
        tls.plain().received(),
        "",
        "no request reached the collector"
    );
}

#[test]
fn test_https_without_a_connector_is_refused_before_dialing() {
    let dial = CollectorDial::answering(OK_EMPTY);
    let client = OtlpHttpClient::over(dial.clone(), TIMEOUT);
    assert!(client.tls().is_none());
    let error = send(&client, post("https://collector/v1/traces", b"x")).expect_err("no connector");
    assert_eq!(
        error.to_string(),
        "https needs a TLS connector: https://collector/v1/traces"
    );
    assert!(dial.dialed().is_empty());
}

#[test]
fn test_a_scheme_other_than_http_or_https_is_refused_before_dialing() {
    let dial = CollectorDial::answering(OK_EMPTY);
    let error = send(
        &OtlpHttpClient::over(dial.clone(), TIMEOUT),
        post("ftp://collector/v1/traces", b"x"),
    )
    .expect_err("not HTTP");
    assert_eq!(
        error.to_string(),
        "http:// or https:// only: ftp://collector/v1/traces"
    );
    assert!(dial.dialed().is_empty());
}

#[test]
fn test_a_refused_tcp_connection_is_an_error_naming_the_address() {
    let port = TcpListener::bind("127.0.0.1:0")
        .expect("a loopback port")
        .local_addr()
        .expect("its address")
        .port();
    // The listener is dropped: nothing accepts on the port.
    let error = send(
        &OtlpHttpClient::new(TIMEOUT),
        post(&format!("http://127.0.0.1:{port}/v1/traces"), b"x"),
    )
    .expect_err("refused");
    assert!(
        error
            .to_string()
            .starts_with(&format!("127.0.0.1:{port}: ")),
        "{error}"
    );
}

#[test]
fn test_a_tcp_server_that_never_answers_fails_the_read_at_the_timeout() {
    // The listener never accepts: the kernel completes the connection into its backlog and takes the request, and
    // no response ever comes, so the client's read waits out its timeout.
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let client = OtlpHttpClient::new(Duration::from_millis(200));
    assert_eq!(client.timeout(), Duration::from_millis(200));
    let error = send(
        &client,
        post(&format!("http://127.0.0.1:{port}/v1/traces"), b"x"),
    )
    .expect_err("no response");
    let kind = error.downcast_ref::<io::Error>().map(io::Error::kind);
    assert!(
        matches!(
            kind,
            Some(io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut)
        ),
        "{error}"
    );
    drop(listener);
}

#[test]
fn test_the_tcp_dial_opens_a_connection_with_its_timeouts() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let stream: TcpStream = OtlpHttpClient::new(TIMEOUT)
        .dial()
        .dial("127.0.0.1", port, Duration::from_millis(300))
        .expect("a connection");
    assert_eq!(
        stream.read_timeout().ok().flatten(),
        Some(Duration::from_millis(300))
    );
    assert_eq!(
        stream.write_timeout().ok().flatten(),
        Some(Duration::from_millis(300))
    );
    drop(listener);
}
