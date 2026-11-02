/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The transport-tls scenarios, bound to `features/transport_tls/`.

use std::io::{Read, Write};
use std::path::PathBuf;

use cucumber::{World, then, when};
use ultragraphdb_transport_tls::{
    TestEcho, TestPki, TlsConnector, TlsError, TlsErrorEnum, TlsTrust,
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
}

/// A file of `contents` named for `name` in the scenario's temporary directory.
fn temp_file(name: &str, contents: &[u8]) -> String {
    let dir = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let path = dir.join(format!("bdd-{name}-{}.pem", std::process::id()));
    std::fs::write(&path, contents).expect("a temporary file");
    path.to_str().expect("a UTF-8 path").to_string()
}

/// Connects to an in-memory echo server of `server_pki` trusting `trusted` for `host`: the echo, or the refusal.
fn connect(w: &mut Steps, server_pki: &TestPki, trusted: &TestPki, host: &str) {
    let trust = TlsTrust::from_pem("trusted", trusted.ca_pem().as_bytes()).expect("one CA");
    match TlsConnector::new(trust).connect(server_pki.peer(TestEcho::default()), host) {
        Ok(mut tls) => {
            tls.write_all(b"ping\n").expect("a write");
            let mut echoed = [0u8; 5];
            tls.read_exact(&mut echoed).expect("the echo");
            w.echoed = Some(echoed);
        }
        Err(error) => w.refused = Some(error),
    }
}

#[when("SSL_CERT_FILE names a bundle that holds a test CA")]
fn ssl_cert_file_names_a_bundle(w: &mut Steps) {
    w.path = temp_file("ssl-cert-file", TestPki::new().ca_pem().as_bytes());
    w.trust = Some(TlsTrust::system_from(Some(&w.path)));
}

#[then("the system trust store holds that one certificate and names that file as its source")]
fn system_trust_is_that_file(w: &mut Steps) {
    let trust = w.trust.take().expect("a read").expect("the bundle");
    assert_eq!((trust.len(), trust.source()), (1, w.path.as_str()));
}

#[when("a bundle lists one CA, another CA, and the first again")]
fn a_bundle_repeats_a_ca(w: &mut Steps) {
    let (one, two) = (TestPki::new(), TestPki::new());
    let bundle = format!("{}{}{}", one.ca_pem(), two.ca_pem(), one.ca_pem());
    w.trust = Some(TlsTrust::from_pem("bundle", bundle.as_bytes()));
}

#[then("the trust store holds two certificates")]
fn two_certificates(w: &mut Steps) {
    let trust = w.trust.take().expect("a read").expect("two certificates");
    assert_eq!(trust.len(), 2);
}

#[when("a bundle holds only a block that is no certificate")]
fn a_bundle_without_a_certificate(w: &mut Steps) {
    w.path = temp_file("empty", NOT_A_CERTIFICATE.as_bytes());
    w.trust = Some(TlsTrust::from_pem_file(&w.path));
}

#[then("reading it is refused as an empty trust store")]
fn refused_as_empty(w: &mut Steps) {
    let error = w.trust.take().expect("a read").expect_err("nothing usable");
    assert_eq!(error.0, TlsErrorEnum::TrustStoreEmpty(w.path.clone()));
}

#[when(
    "a client trusting the test CA connects to a server presenting the certificate it issued for `localhost`"
)]
fn trusted_client_connects(w: &mut Steps) {
    let pki = TestPki::new();
    connect(w, &pki, &pki, "localhost");
}

#[then("the handshake verifies and a line sent comes back from the server")]
fn line_comes_back(w: &mut Steps) {
    assert_eq!(w.refused, None);
    assert_eq!(w.echoed, Some(*b"ping\n"));
}

#[when("a client trusting another CA connects to that server")]
fn untrusting_client_connects(w: &mut Steps) {
    connect(w, &TestPki::new(), &TestPki::new(), "localhost");
}

#[then("the connection is refused as a failed handshake naming an unknown issuer")]
fn refused_unknown_issuer(w: &mut Steps) {
    // rustls's text for a chain to no trusted anchor (`CertificateError::UnknownIssuer`'s Display).
    let error = w.refused.take().expect("a refusal");
    assert!(
        matches!(&error.0, TlsErrorEnum::HandshakeFailed { reason, .. } if reason.contains("UnknownIssuer")),
        "{error:?}"
    );
}

#[when("a client trusting the test CA connects to that server for `collector.example`")]
fn client_connects_for_another_name(w: &mut Steps) {
    let pki = TestPki::new();
    connect(w, &pki, &pki, "collector.example");
}

#[then(
    "the connection is refused as a failed handshake naming a certificate not valid for the name"
)]
fn refused_not_valid_for_name(w: &mut Steps) {
    // rustls's text for a certificate whose names exclude the host (`CertificateError::NotValidForNameContext`'s
    // Display).
    let error = w.refused.take().expect("a refusal");
    assert!(
        matches!(&error.0, TlsErrorEnum::HandshakeFailed { reason, .. } if reason.contains("certificate not valid for name")),
        "{error:?}"
    );
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()),
    );
}
