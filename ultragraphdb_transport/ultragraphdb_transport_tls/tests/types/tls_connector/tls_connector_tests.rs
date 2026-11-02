/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rustls::ProtocolVersion;
use ultragraphdb_transport_tls::{TestPki, TestServerStream, TlsConnector, TlsErrorEnum, TlsTrust};

/// A loopback server presenting `pki`'s certificate to one connection: it reads five bytes and echoes them. The
/// thread returns what it read, or `None` when the client gave the handshake up.
fn serve_once(pki: &TestPki) -> (u16, JoinHandle<Option<[u8; 5]>>) {
    serve_once_with(pki, TestPki::accept)
}

/// A loopback server as [`serve_once`] runs it, its server side made by `accept`.
#[expect(
    clippy::disallowed_methods,
    reason = "a test's TLS server runs on a thread of its own"
)]
fn serve_once_with(
    pki: &TestPki,
    accept: fn(&TestPki, TcpStream) -> TestServerStream,
) -> (u16, JoinHandle<Option<[u8; 5]>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let pki = pki.clone();
    let handle = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("one connection");
        let mut tls = accept(&pki, stream);
        let mut line = [0u8; 5];
        tls.read_exact(&mut line).ok()?;
        tls.write_all(&line).ok()?;
        tls.flush().ok()?;
        Some(line)
    });
    (port, handle)
}

/// A socket to the loopback `port` whose reads and writes wait at most `timeout`.
fn socket(port: u16, timeout: Duration) -> TcpStream {
    let stream = TcpStream::connect(("127.0.0.1", port)).expect("the server accepts");
    stream.set_read_timeout(Some(timeout)).expect("a timeout");
    stream.set_write_timeout(Some(timeout)).expect("a timeout");
    stream
}

fn connector(pki: &TestPki) -> TlsConnector {
    TlsConnector::new(TlsTrust::from_pem("test CA", pki.ca_pem().as_bytes()).expect("the CA"))
}

#[test]
fn test_a_trusted_certificate_verifies_by_name_and_by_address() {
    let pki = TestPki::new();
    for host in ["localhost", "127.0.0.1"] {
        let (port, server) = serve_once(&pki);
        let mut tls = connector(&pki)
            .connect(socket(port, Duration::from_secs(5)), host)
            .expect("the handshake verifies");
        // The client offers TLS 1.3 and 1.2 and the server takes both, so they agree on 1.3.
        assert_eq!(
            tls.conn.protocol_version(),
            Some(ProtocolVersion::TLSv1_3),
            "{host}"
        );
        assert!(
            !tls.conn.is_handshaking(),
            "{host}: complete before the first write"
        );
        tls.write_all(b"ping\n").expect("a write");
        let mut echoed = [0u8; 5];
        tls.read_exact(&mut echoed).expect("the echo");
        assert_eq!(&echoed, b"ping\n", "{host}");
        assert_eq!(server.join().expect("the server"), Some(*b"ping\n"));
    }
}

#[test]
fn test_a_server_speaking_tls_1_2_alone_is_verified_over_tls_1_2() {
    let pki = TestPki::new();
    let (port, server) = serve_once_with(&pki, TestPki::accept_tls12);
    let mut tls = connector(&pki)
        .connect(socket(port, Duration::from_secs(5)), "localhost")
        .expect("the handshake verifies over TLS 1.2");
    assert_eq!(tls.conn.protocol_version(), Some(ProtocolVersion::TLSv1_2));
    tls.write_all(b"ping\n").expect("a write");
    let mut echoed = [0u8; 5];
    tls.read_exact(&mut echoed).expect("the echo");
    assert_eq!(server.join().expect("the server"), Some(*b"ping\n"));
}

#[test]
fn test_a_certificate_from_an_untrusted_ca_fails_the_handshake() {
    let (pki, other) = (TestPki::new(), TestPki::new());
    let (port, server) = serve_once(&pki);
    let error = connector(&other)
        .connect(socket(port, Duration::from_secs(5)), "localhost")
        .expect_err("another CA's certificate");
    // rustls's text for a chain to no trusted anchor (`CertificateError::UnknownIssuer`'s Display).
    assert!(
        matches!(&error.0, TlsErrorEnum::HandshakeFailed { host, reason }
            if host == "localhost" && reason.contains("UnknownIssuer")),
        "{error:?}"
    );
    assert_eq!(server.join().expect("the server"), None);
}

#[test]
fn test_a_certificate_for_another_name_fails_the_handshake() {
    let pki = TestPki::new();
    let (port, server) = serve_once(&pki);
    let error = connector(&pki)
        .connect(socket(port, Duration::from_secs(5)), "collector.example")
        .expect_err("the certificate names localhost and 127.0.0.1");
    // rustls's text for a certificate whose names exclude the host (`CertificateError::NotValidForNameContext`'s
    // Display).
    assert!(
        matches!(&error.0, TlsErrorEnum::HandshakeFailed { host, reason }
            if host == "collector.example" && reason.contains("certificate not valid for name")),
        "{error:?}"
    );
    assert_eq!(server.join().expect("the server"), None);
}

#[test]
fn test_a_host_no_certificate_can_name_is_refused_before_the_handshake() {
    let pki = TestPki::new();
    for host in ["not a host", ""] {
        let (port, server) = serve_once(&pki);
        let error = connector(&pki)
            .connect(socket(port, Duration::from_secs(5)), host)
            .expect_err("no such name");
        assert_eq!(error.0, TlsErrorEnum::InvalidServerName(host.to_string()));
        assert_eq!(server.join().expect("the server"), None);
    }
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "a silent server holds its connection on a thread of its own"
)]
fn test_a_server_that_never_answers_fails_the_handshake_at_the_sockets_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("one connection");
        thread::sleep(Duration::from_millis(800));
        drop(stream);
    });
    let pki = TestPki::new();
    let error = connector(&pki)
        .connect(socket(port, Duration::from_millis(200)), "localhost")
        .expect_err("no server hello");
    assert!(
        matches!(&error.0, TlsErrorEnum::HandshakeFailed { host, .. } if host == "localhost"),
        "{error:?}"
    );
    server.join().expect("the server");
}

#[test]
fn test_the_connector_keeps_its_trust() {
    let pki = TestPki::new();
    let kept = connector(&pki);
    assert_eq!((kept.trust().source(), kept.trust().len()), ("test CA", 1));
}
