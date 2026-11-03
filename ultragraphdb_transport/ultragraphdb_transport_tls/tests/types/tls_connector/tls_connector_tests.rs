/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use rustls::ProtocolVersion;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transport_tls::{TestEcho, TestPki, TlsConnector, TlsErrorEnum, TlsTrust};

/// A connector trusting `pki`'s authority alone.
fn connector(pki: &TestPki) -> TestResult<TlsConnector> {
    let trust = TlsTrust::from_pem("test CA", pki.ca_pem().as_bytes()).context("the CA")?;
    Ok(TlsConnector::new(trust)?)
}

#[test]
fn test_a_trusted_certificate_verifies_by_name_and_by_address() -> TestResult {
    let pki = TestPki::new()?;
    for host in ["localhost", "127.0.0.1"] {
        let mut tls = connector(&pki)?
            .connect(pki.peer(TestEcho::default())?, host)
            .context("the handshake verifies")?;
        assert!(
            !tls.session().is_handshaking(),
            "{host}: complete before the first write"
        );
        // The client offers TLS 1.3 and 1.2 and the server takes both, so they agree on 1.3.
        assert_eq!(
            tls.session().protocol_version(),
            Some(ProtocolVersion::TLSv1_3),
            "{host}"
        );
        tls.write_all(b"ping\n").context("a write")?;
        let mut echoed = [0u8; 5];
        tls.read_exact(&mut echoed).context("the echo")?;
        assert_eq!(&echoed, b"ping\n", "{host}");
        assert!(!tls.get_ref().failed(), "{host}");
    }
    Ok(())
}

#[test]
fn test_a_server_speaking_tls_1_2_alone_is_verified_over_tls_1_2() -> TestResult {
    let pki = TestPki::new()?;
    let mut tls = connector(&pki)?
        .connect(pki.peer_tls12(TestEcho::default())?, "localhost")
        .context("the handshake verifies over TLS 1.2")?;
    assert_eq!(
        tls.session().protocol_version(),
        Some(ProtocolVersion::TLSv1_2)
    );
    tls.write_all(b"ping\n").context("a write")?;
    let mut echoed = [0u8; 5];
    tls.read_exact(&mut echoed).context("the echo")?;
    assert_eq!(&echoed, b"ping\n");
    Ok(())
}

#[test]
fn test_a_certificate_from_an_untrusted_ca_fails_the_handshake() -> TestResult {
    let (pki, other) = (TestPki::new()?, TestPki::new()?);
    let Err(error) = connector(&other)?.connect(pki.peer(TestEcho::default())?, "localhost") else {
        return Err(TestError::new("another CA's certificate verified"));
    };
    // rustls's text for a chain to no trusted anchor (`CertificateError::UnknownIssuer`'s Display).
    assert!(
        matches!(&error.0, TlsErrorEnum::HandshakeFailed { host, reason }
            if host == "localhost" && reason.contains("UnknownIssuer")),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn test_a_certificate_for_another_name_fails_the_handshake() -> TestResult {
    let pki = TestPki::new()?;
    let Err(error) = connector(&pki)?.connect(pki.peer(TestEcho::default())?, "collector.example")
    else {
        return Err(TestError::new(
            "a certificate naming localhost and 127.0.0.1 verified for collector.example",
        ));
    };
    // rustls's text for a certificate whose names exclude the host (`CertificateError::NotValidForNameContext`'s
    // Display).
    assert!(
        matches!(&error.0, TlsErrorEnum::HandshakeFailed { host, reason }
            if host == "collector.example" && reason.contains("certificate not valid for name")),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn test_a_host_no_certificate_can_name_is_refused_before_the_handshake() -> TestResult {
    let pki = TestPki::new()?;
    for host in ["not a host", ""] {
        let Err(error) = connector(&pki)?.connect(pki.peer(TestEcho::default())?, host) else {
            return Err(TestError::new(format!(
                "{host:?} was taken as a server name"
            )));
        };
        assert_eq!(error.0, TlsErrorEnum::InvalidServerName(host.to_string()));
    }
    Ok(())
}

#[test]
fn test_a_server_that_never_answers_fails_the_handshake_at_the_sockets_timeout() -> TestResult {
    // The listener never accepts: the kernel completes the connection into its backlog and takes the client hello,
    // and no server hello ever comes, so the client's read waits out its timeout.
    let listener = TcpListener::bind("127.0.0.1:0").context("a loopback port")?;
    let port = listener.local_addr().context("its address")?.port();
    let socket =
        TcpStream::connect(("127.0.0.1", port)).context("the backlog takes the connection")?;
    socket
        .set_read_timeout(Some(Duration::from_millis(200)))
        .context("a timeout")?;
    let pki = TestPki::new()?;
    let Err(error) = connector(&pki)?.connect(socket, "localhost") else {
        return Err(TestError::new(
            "a handshake without a server hello verified",
        ));
    };
    assert!(
        matches!(&error.0, TlsErrorEnum::HandshakeFailed { host, .. } if host == "localhost"),
        "{error:?}"
    );
    drop(listener);
    Ok(())
}

#[test]
fn test_the_connector_keeps_its_trust() -> TestResult {
    let pki = TestPki::new()?;
    let kept = connector(&pki)?;
    assert_eq!((kept.trust().source(), kept.trust().len()), ("test CA", 1));
    Ok(())
}

#[test]
fn test_a_session_verifies_for_its_host_and_refuses_a_host_no_certificate_names() -> TestResult {
    let pki = TestPki::new()?;
    let session = connector(&pki)?.session("localhost")?;
    assert_eq!(session.host(), "localhost");
    assert!(session.is_handshaking());
    assert!(session.wants_write(), "the client hello waits");
    let Err(error) = connector(&pki)?.session("not a host") else {
        return Err(TestError::new("\"not a host\" was taken as a server name"));
    };
    assert_eq!(
        error.0,
        TlsErrorEnum::InvalidServerName("not a host".to_string())
    );
    Ok(())
}
