/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use rustls::ProtocolVersion;
use ultragraphdb_transport_tls::{TestEcho, TestPki, TlsConnector, TlsErrorEnum, TlsTrust};

fn connector(pki: &TestPki) -> TlsConnector {
    TlsConnector::new(TlsTrust::from_pem("test CA", pki.ca_pem().as_bytes()).expect("the CA"))
}

#[test]
fn test_a_trusted_certificate_verifies_by_name_and_by_address() {
    let pki = TestPki::new();
    for host in ["localhost", "127.0.0.1"] {
        let mut tls = connector(&pki)
            .connect(pki.peer(TestEcho::default()), host)
            .expect("the handshake verifies");
        assert!(
            !tls.conn.is_handshaking(),
            "{host}: complete before the first write"
        );
        // The client offers TLS 1.3 and 1.2 and the server takes both, so they agree on 1.3.
        assert_eq!(
            tls.conn.protocol_version(),
            Some(ProtocolVersion::TLSv1_3),
            "{host}"
        );
        tls.write_all(b"ping\n").expect("a write");
        let mut echoed = [0u8; 5];
        tls.read_exact(&mut echoed).expect("the echo");
        assert_eq!(&echoed, b"ping\n", "{host}");
        assert!(!tls.sock.failed(), "{host}");
    }
}

#[test]
fn test_a_server_speaking_tls_1_2_alone_is_verified_over_tls_1_2() {
    let pki = TestPki::new();
    let mut tls = connector(&pki)
        .connect(pki.peer_tls12(TestEcho::default()), "localhost")
        .expect("the handshake verifies over TLS 1.2");
    assert_eq!(tls.conn.protocol_version(), Some(ProtocolVersion::TLSv1_2));
    tls.write_all(b"ping\n").expect("a write");
    let mut echoed = [0u8; 5];
    tls.read_exact(&mut echoed).expect("the echo");
    assert_eq!(&echoed, b"ping\n");
}

#[test]
fn test_a_certificate_from_an_untrusted_ca_fails_the_handshake() {
    let (pki, other) = (TestPki::new(), TestPki::new());
    let error = connector(&other)
        .connect(pki.peer(TestEcho::default()), "localhost")
        .expect_err("another CA's certificate");
    // rustls's text for a chain to no trusted anchor (`CertificateError::UnknownIssuer`'s Display).
    assert!(
        matches!(&error.0, TlsErrorEnum::HandshakeFailed { host, reason }
            if host == "localhost" && reason.contains("UnknownIssuer")),
        "{error:?}"
    );
}

#[test]
fn test_a_certificate_for_another_name_fails_the_handshake() {
    let pki = TestPki::new();
    let error = connector(&pki)
        .connect(pki.peer(TestEcho::default()), "collector.example")
        .expect_err("the certificate names localhost and 127.0.0.1");
    // rustls's text for a certificate whose names exclude the host (`CertificateError::NotValidForNameContext`'s
    // Display).
    assert!(
        matches!(&error.0, TlsErrorEnum::HandshakeFailed { host, reason }
            if host == "collector.example" && reason.contains("certificate not valid for name")),
        "{error:?}"
    );
}

#[test]
fn test_a_host_no_certificate_can_name_is_refused_before_the_handshake() {
    let pki = TestPki::new();
    for host in ["not a host", ""] {
        let error = connector(&pki)
            .connect(pki.peer(TestEcho::default()), host)
            .expect_err("no such name");
        assert_eq!(error.0, TlsErrorEnum::InvalidServerName(host.to_string()));
    }
}

#[test]
fn test_a_server_that_never_answers_fails_the_handshake_at_the_sockets_timeout() {
    // The listener never accepts: the kernel completes the connection into its backlog and takes the client hello,
    // and no server hello ever comes, so the client's read waits out its timeout.
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let socket = TcpStream::connect(("127.0.0.1", port)).expect("the backlog takes the connection");
    socket
        .set_read_timeout(Some(Duration::from_millis(200)))
        .expect("a timeout");
    let pki = TestPki::new();
    let error = connector(&pki)
        .connect(socket, "localhost")
        .expect_err("no server hello");
    assert!(
        matches!(&error.0, TlsErrorEnum::HandshakeFailed { host, .. } if host == "localhost"),
        "{error:?}"
    );
    drop(listener);
}

#[test]
fn test_the_connector_keeps_its_trust() {
    let pki = TestPki::new();
    let kept = connector(&pki);
    assert_eq!((kept.trust().source(), kept.trust().len()), ("test CA", 1));
}
