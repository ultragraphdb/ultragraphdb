/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt::Display;
use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use rustls::{ServerConfig, ServerConnection, SupportedProtocolVersion};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};

use crate::errors::tls_error::{TlsError, TlsErrorEnum};
use crate::utils_test::test_peer::TestPeer;

/// The number of the next authority, so no two authorities of a process share a name: a verifier finding a CA of
/// the issuer's name checks its signature, which reports a wrong signature rather than an unknown issuer.
// An atomic, not a `Cell`: tests create authorities on several threads at once.
static NEXT_CA: AtomicU64 = AtomicU64::new(0);

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
    ///
    /// # Errors
    ///
    /// [`TlsErrorEnum::ConfigurationRefused`] if rcgen cannot generate a key or sign a certificate, or rustls refuses
    /// the server configuration.
    pub fn new() -> Result<Self, TlsError> {
        let ca_key = KeyPair::generate().map_err(refused)?;
        let mut ca = CertificateParams::new(Vec::<String>::new()).map_err(refused)?;
        ca.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        // Relaxed suffices: the counter orders nothing, it only has to hand out each number once.
        let number = NEXT_CA.fetch_add(1, Ordering::Relaxed);
        ca.distinguished_name
            .push(DnType::CommonName, format!("ultragraphdb test CA {number}"));
        let ca_pem = ca.self_signed(&ca_key).map_err(refused)?.pem();
        let issuer = Issuer::new(ca, ca_key);

        let server_key = KeyPair::generate().map_err(refused)?;
        let mut server =
            CertificateParams::new(vec!["localhost".to_string(), "127.0.0.1".to_string()])
                .map_err(refused)?;
        server.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let server_cert = server.signed_by(&server_key, &issuer).map_err(refused)?;
        let key = PrivateKeyDer::from_pem_slice(server_key.serialize_pem().as_bytes())
            .map_err(refused)?;
        let chain = vec![server_cert.der().clone()];
        Ok(Self {
            ca_pem,
            server: server_config(
                &[&rustls::version::TLS13, &rustls::version::TLS12],
                chain.clone(),
                key.clone_key(),
            )?,
            server_tls12: server_config(&[&rustls::version::TLS12], chain, key)?,
        })
    }

    /// The authority's certificate in PEM, for a [`TlsTrust`](crate::TlsTrust) that verifies the server.
    #[must_use]
    pub fn ca_pem(&self) -> &str {
        &self.ca_pem
    }

    /// An in-memory server presenting the server certificate over TLS 1.3 or 1.2, in front of `application`.
    ///
    /// # Errors
    ///
    /// [`TlsErrorEnum::ConfigurationRefused`] if rustls cannot open a connection of the server configuration.
    pub fn peer<A: Read + Write>(&self, application: A) -> Result<TestPeer<A>, TlsError> {
        Ok(TestPeer::new(connection(&self.server)?, application))
    }

    /// An in-memory server as [`Self::peer`] gives it, speaking TLS 1.2 alone.
    ///
    /// # Errors
    ///
    /// As [`Self::peer`].
    pub fn peer_tls12<A: Read + Write>(&self, application: A) -> Result<TestPeer<A>, TlsError> {
        Ok(TestPeer::new(connection(&self.server_tls12)?, application))
    }
}

/// A server connection of `config`.
fn connection(config: &Arc<ServerConfig>) -> Result<ServerConnection, TlsError> {
    ServerConnection::new(Arc::clone(config)).map_err(refused)
}

/// [`TlsErrorEnum::ConfigurationRefused`] for what rcgen or rustls reported.
fn refused(error: impl Display) -> TlsError {
    TlsError(TlsErrorEnum::ConfigurationRefused(error.to_string()))
}

/// A server configuration presenting `chain` with `key` over `versions`, ring as its provider.
fn server_config(
    versions: &[&'static SupportedProtocolVersion],
    chain: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Result<Arc<ServerConfig>, TlsError> {
    let config =
        ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_protocol_versions(versions)
            .map_err(refused)?
            .with_no_client_auth()
            .with_single_cert(chain, key)
            .map_err(refused)?;
    Ok(Arc::new(config))
}
