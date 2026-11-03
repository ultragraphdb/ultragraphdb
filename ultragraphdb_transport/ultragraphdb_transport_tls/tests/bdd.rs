/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The transport-tls scenarios, bound to `features/transport_tls/`.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;

use cucumber::{World, then, when};
use rustls::ProtocolVersion;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transport_tls::{
    CertificateChainFile, PrivateKeyFile, TestEcho, TestLink, TestPeer, TestPipe, TestPki,
    TlsAcceptor, TlsConnector, TlsError, TlsErrorEnum, TlsTrust,
};

/// A block in certificate armour whose content is no certificate: the base64 of `not a certificate`.
const NOT_A_CERTIFICATE: &str =
    "-----BEGIN CERTIFICATE-----\nbm90IGEgY2VydGlmaWNhdGU=\n-----END CERTIFICATE-----\n";

#[derive(Debug, Default, World)]
struct Steps {
    path: String,
    trust: Option<Result<TlsTrust, TlsError>>,
    echoed: Option<[u8; 5]>,
    refused: Option<TlsError>,
    version: Option<ProtocolVersion>,
    accepted: Option<Result<(), TlsError>>,
    client_failed: bool,
    built: Option<Result<TlsAcceptor, TlsError>>,
    exchanges: Vec<Vec<u8>>,
}

/// A file of `contents` named for `name` in the scenario's temporary directory.
fn temp_file(name: &str, contents: &[u8]) -> TestResult<String> {
    let dir = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let path = dir.join(format!("bdd-{name}-{}.pem", std::process::id()));
    std::fs::write(&path, contents).context("a temporary file")?;
    Ok(path.to_str().context("a UTF-8 path")?.to_string())
}

/// Connects to an in-memory echo server of `server_pki` trusting `trusted` for `host`: the echo, or the refusal.
fn connect(w: &mut Steps, server_pki: &TestPki, trusted: &TestPki, host: &str) -> TestResult {
    let trust = TlsTrust::from_pem("trusted", trusted.ca_pem().as_bytes()).context("one CA")?;
    match TlsConnector::new(trust)?.connect(server_pki.peer(TestEcho::default())?, host) {
        Ok(mut tls) => {
            tls.write_all(b"ping\n").context("a write")?;
            let mut echoed = [0u8; 5];
            tls.read_exact(&mut echoed).context("the echo")?;
            w.echoed = Some(echoed);
        }
        Err(error) => w.refused = Some(error),
    }
    Ok(())
}

#[when("SSL_CERT_FILE names a bundle that holds a test CA")]
fn ssl_cert_file_names_a_bundle(w: &mut Steps) -> TestResult {
    w.path = temp_file("ssl-cert-file", TestPki::new()?.ca_pem().as_bytes())?;
    w.trust = Some(TlsTrust::system_from(Some(&w.path)));
    Ok(())
}

#[then("the system trust store holds that one certificate and names that file as its source")]
fn system_trust_is_that_file(w: &mut Steps) -> TestResult {
    let trust = w.trust.take().context("a read")?.context("the bundle")?;
    assert_eq!((trust.len(), trust.source()), (1, w.path.as_str()));
    Ok(())
}

#[when("a bundle lists one CA, another CA, and the first again")]
fn a_bundle_repeats_a_ca(w: &mut Steps) -> TestResult {
    let (one, two) = (TestPki::new()?, TestPki::new()?);
    let bundle = format!("{}{}{}", one.ca_pem(), two.ca_pem(), one.ca_pem());
    w.trust = Some(TlsTrust::from_pem("bundle", bundle.as_bytes()));
    Ok(())
}

#[then("the trust store holds two certificates")]
fn two_certificates(w: &mut Steps) -> TestResult {
    let trust = w
        .trust
        .take()
        .context("a read")?
        .context("two certificates")?;
    assert_eq!(trust.len(), 2);
    Ok(())
}

#[when("a bundle holds only a block that is no certificate")]
fn a_bundle_without_a_certificate(w: &mut Steps) -> TestResult {
    w.path = temp_file("empty", NOT_A_CERTIFICATE.as_bytes())?;
    w.trust = Some(TlsTrust::from_pem_file(&w.path));
    Ok(())
}

#[then("reading it is refused as an empty trust store")]
fn refused_as_empty(w: &mut Steps) -> TestResult {
    let Err(error) = w.trust.take().context("a read")? else {
        return Err(TestError::new(
            "a bundle with nothing usable was read as a trust store",
        ));
    };
    assert_eq!(error.0, TlsErrorEnum::TrustStoreEmpty(w.path.clone()));
    Ok(())
}

#[when(
    "a client trusting the test CA connects to a server presenting the certificate it issued for `localhost`"
)]
fn trusted_client_connects(w: &mut Steps) -> TestResult {
    let pki = TestPki::new()?;
    connect(w, &pki, &pki, "localhost")
}

#[then("the handshake verifies and a line sent comes back from the server")]
fn line_comes_back(w: &mut Steps) -> TestResult {
    assert_eq!(w.refused, None);
    assert_eq!(w.echoed, Some(*b"ping\n"));
    Ok(())
}

#[when("a client trusting another CA connects to that server")]
fn untrusting_client_connects(w: &mut Steps) -> TestResult {
    connect(w, &TestPki::new()?, &TestPki::new()?, "localhost")
}

#[then("the connection is refused as a failed handshake naming an unknown issuer")]
fn refused_unknown_issuer(w: &mut Steps) -> TestResult {
    // rustls's text for a chain to no trusted anchor (`CertificateError::UnknownIssuer`'s Display).
    let error = w.refused.take().context("a refusal")?;
    assert!(
        matches!(&error.0, TlsErrorEnum::HandshakeFailed { reason, .. } if reason.contains("UnknownIssuer")),
        "{error:?}"
    );
    Ok(())
}

#[when("a client trusting the test CA connects to that server for `collector.example`")]
fn client_connects_for_another_name(w: &mut Steps) -> TestResult {
    let pki = TestPki::new()?;
    connect(w, &pki, &pki, "collector.example")
}

#[then(
    "the connection is refused as a failed handshake naming a certificate not valid for the name"
)]
fn refused_not_valid_for_name(w: &mut Steps) -> TestResult {
    // rustls's text for a certificate whose names exclude the host (`CertificateError::NotValidForNameContext`'s
    // Display).
    let error = w.refused.take().context("a refusal")?;
    assert!(
        matches!(&error.0, TlsErrorEnum::HandshakeFailed { reason, .. } if reason.contains("certificate not valid for name")),
        "{error:?}"
    );
    Ok(())
}

#[when(
    "a client trusting the test CA connects to an acceptor holding the certificate the CA issued for `localhost`"
)]
fn client_connects_to_an_acceptor(w: &mut Steps) -> TestResult {
    let pki = TestPki::new()?;
    let server = TestPeer::new(pki.acceptor()?.session()?, TestEcho::default());
    let mut tls = pki.connector()?.connect(server, "localhost")?;
    w.version = tls.session().protocol_version();
    tls.write_all(b"ping\n").context("a write")?;
    let mut echoed = [0u8; 5];
    tls.read_exact(&mut echoed).context("the echo")?;
    w.echoed = Some(echoed);
    Ok(())
}

#[then("the handshake completes with TLS 1.3 and a line sent comes back from the server")]
fn tls_1_3_and_the_line_comes_back(w: &mut Steps) -> TestResult {
    assert_eq!(w.version, Some(ProtocolVersion::TLSv1_3));
    assert_eq!(w.echoed, Some(*b"ping\n"));
    Ok(())
}

#[when("a client offering TLS 1.2 alone connects to the acceptor")]
fn tls_1_2_client_connects(w: &mut Steps) -> TestResult {
    let pki = TestPki::new()?;
    let mut client = TestPeer::new(pki.client_tls12("localhost")?, TestEcho::default());
    w.accepted = Some(pki.acceptor()?.accept(&mut client).map(|_| ()));
    w.client_failed = client.failed();
    Ok(())
}

#[then("the handshake fails as a protocol-version refusal, and the caller reads no byte")]
fn protocol_version_refusal(w: &mut Steps) -> TestResult {
    let Err(error) = w.accepted.take().context("an accept")? else {
        return Err(TestError::new(
            "the acceptor gave its caller a stream to read",
        ));
    };
    // rustls's text for a client hello offering no version the server enables, which it answers with the
    // protocol_version alert (RFC 8446 §4.2.1, §6.2).
    assert!(
        matches!(&error.0, TlsErrorEnum::HandshakeFailed { host, reason }
            if host.is_empty() && reason.contains("Tls12NotOfferedOrEnabled")),
        "{error:?}"
    );
    assert!(w.client_failed, "the client received the alert");
    Ok(())
}

#[when("an acceptor is built from the test CA's server certificate and another key")]
fn acceptor_with_another_key(w: &mut Steps) -> TestResult {
    let pki = TestPki::new()?;
    let chain = temp_file("chain", pki.server_chain_pem().as_bytes())?;
    w.path = temp_file("other-key", pki.other_key_pem().as_bytes())?;
    w.built = Some(TlsAcceptor::from_pem_files(
        &CertificateChainFile::new(chain),
        &PrivateKeyFile::new(w.path.as_str()),
    ));
    Ok(())
}

#[then("construction fails with `KeyMismatch`")]
fn fails_with_key_mismatch(w: &mut Steps) -> TestResult {
    let Err(error) = w.built.take().context("a construction")? else {
        return Err(TestError::new(
            "an acceptor was built from a key of another certificate",
        ));
    };
    assert_eq!(error.0, TlsErrorEnum::KeyMismatch(w.path.clone()));
    Ok(())
}

#[when(
    "a client and a server complete a handshake and exchange one line through the session over an in-memory pipe, and again over a TCP socket"
)]
fn exchange_over_two_carriers(w: &mut Steps) -> TestResult {
    let pki = TestPki::new()?;
    let (near, far) = TestPipe::pair();
    let mut client = TestLink::new(pki.connector()?.session("localhost")?, near);
    let mut server = TestLink::new(pki.acceptor()?.session()?, far);
    w.exchanges.push(TestLink::exchange_line(
        &mut client,
        &mut server,
        b"line\n",
    )?);

    let listener = TcpListener::bind("127.0.0.1:0").context("a loopback port")?;
    let near_tcp = TcpStream::connect(listener.local_addr().context("its address")?)
        .context("the backlog takes the connection")?;
    let (far_tcp, _) = listener.accept().context("the connection")?;
    near_tcp.set_nonblocking(true).context("non-blocking")?;
    far_tcp.set_nonblocking(true).context("non-blocking")?;
    let mut client_tcp = TestLink::new(pki.connector()?.session("localhost")?, near_tcp);
    let mut server_tcp = TestLink::new(pki.acceptor()?.session()?, far_tcp);
    w.exchanges.push(TestLink::exchange_line(
        &mut client_tcp,
        &mut server_tcp,
        b"line\n",
    )?);
    Ok(())
}

#[then("both exchanges succeed, and the session's code is the same in both")]
fn both_exchanges_succeed(w: &mut Steps) -> TestResult {
    // One `TestLink::exchange_line` over `TlsSession` ran both: only the carrier's type differed.
    assert_eq!(w.exchanges, vec![b"line\n".to_vec(), b"line\n".to_vec()]);
    Ok(())
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
