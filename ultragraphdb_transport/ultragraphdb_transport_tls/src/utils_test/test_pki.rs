/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use rustls::{ServerConfig, ServerConnection, StreamOwned, SupportedProtocolVersion};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};

/// The number of the next authority, so no two authorities of a process share a name: a verifier finding a CA of
/// the issuer's name checks its signature, which reports a wrong signature rather than an unknown issuer.
// An atomic, not a `Cell`: tests create authorities on several threads at once.
static NEXT_CA: AtomicU64 = AtomicU64::new(0);

/// The server side of a test connection, read and written as any other stream.
pub type TestServerStream = StreamOwned<ServerConnection, TcpStream>;

/// A certificate authority and a server certificate it issued for `localhost` and `127.0.0.1`, generated with
/// ECDSA P-256 keys by rcgen on every call, so a test verifies a real handshake against a CA no other test shares
/// and no tool outside the build creates. Each authority is named `ultragraphdb test CA <n>`, `n` unique in the
/// process.
#[derive(Debug, Clone)]
pub struct TestPki {
    ca_pem: String,
    // Shared, not copied: rustls takes its configuration as an `Arc` for every connection it accepts.
    server: Arc<ServerConfig>,
    server_tls12: Arc<ServerConfig>,
}

impl TestPki {
    /// A fresh authority and server certificate.
    #[must_use]
    #[expect(
        clippy::expect_used,
        reason = "each expect names the invariant that keeps a generated key and its own parameters valid"
    )]
    pub fn new() -> Self {
        let ca_key = KeyPair::generate().expect("ring generates a P-256 key");
        let mut ca = CertificateParams::new(Vec::<String>::new()).expect("no names to parse");
        ca.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        // Relaxed suffices: the counter orders nothing, it only has to hand out each number once.
        let number = NEXT_CA.fetch_add(1, Ordering::Relaxed);
        ca.distinguished_name
            .push(DnType::CommonName, format!("ultragraphdb test CA {number}"));
        let ca_pem = ca
            .self_signed(&ca_key)
            .expect("a CA signs its own parameters with its own key")
            .pem();
        let issuer = Issuer::new(ca, ca_key);

        let server_key = KeyPair::generate().expect("ring generates a P-256 key");
        let mut server =
            CertificateParams::new(vec!["localhost".to_string(), "127.0.0.1".to_string()])
                .expect("a DNS name and an IP address parse");
        server.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let server_cert = server
            .signed_by(&server_key, &issuer)
            .expect("the CA signs a certificate for another key it generated");
        let key = PrivateKeyDer::from_pem_slice(server_key.serialize_pem().as_bytes())
            .expect("rcgen writes a PKCS#8 key rustls reads");
        let chain = vec![server_cert.der().clone()];
        Self {
            ca_pem,
            server: server_config(
                &[&rustls::version::TLS13, &rustls::version::TLS12],
                chain.clone(),
                key.clone_key(),
            ),
            server_tls12: server_config(&[&rustls::version::TLS12], chain, key),
        }
    }

    /// The authority's certificate in PEM, for a [`TlsTrust`](crate::TlsTrust) that verifies the server.
    #[must_use]
    pub fn ca_pem(&self) -> &str {
        &self.ca_pem
    }

    /// The server side of `stream`, presenting the server certificate; the handshake runs on its first read or
    /// write.
    #[must_use]
    #[expect(
        clippy::expect_used,
        reason = "a connection from a configuration that built fails only for a configuration error"
    )]
    pub fn accept(&self, stream: TcpStream) -> TestServerStream {
        let connection = ServerConnection::new(Arc::clone(&self.server))
            .expect("a server configuration that built opens a connection");
        StreamOwned::new(connection, stream)
    }

    /// The server side of `stream` as [`Self::accept`] gives it, speaking TLS 1.2 alone.
    #[must_use]
    #[expect(
        clippy::expect_used,
        reason = "a connection from a configuration that built fails only for a configuration error"
    )]
    pub fn accept_tls12(&self, stream: TcpStream) -> TestServerStream {
        let connection = ServerConnection::new(Arc::clone(&self.server_tls12))
            .expect("a server configuration that built opens a connection");
        StreamOwned::new(connection, stream)
    }
}

/// A server configuration presenting `chain` with `key` over `versions`, ring as its provider.
#[expect(
    clippy::expect_used,
    reason = "ring supports both versions, and the chain's certificate was issued for this key"
)]
fn server_config(
    versions: &[&'static SupportedProtocolVersion],
    chain: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Arc<ServerConfig> {
    let config =
        ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_protocol_versions(versions)
            .expect("ring's provider supports TLS 1.3 and 1.2")
            .with_no_client_auth()
            .with_single_cert(chain, key)
            .expect("a P-256 certificate and its own key form a valid server identity");
    Arc::new(config)
}

impl Default for TestPki {
    fn default() -> Self {
        Self::new()
    }
}
