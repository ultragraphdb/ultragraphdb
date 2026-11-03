/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::{Read, Write};
use std::sync::Arc;

use rustls::server::ServerConnection;
use rustls::{Connection, InconsistentKeys, ServerConfig};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};

use crate::errors::tls_error::{TlsError, TlsErrorEnum};
use crate::types::certificate_chain_file::CertificateChainFile;
use crate::types::private_key_file::PrivateKeyFile;
use crate::types::tls_session::TlsSession;
use crate::types::tls_stream::TlsStream;

/// Accepts TLS 1.3 connections presenting one certificate chain with its private key, ring as the crypto provider, no
/// client certificate: TLS 1.3 only, from the connection's first byte, with no clear-text upgrade (`SECURITY.md`,
/// *Encryption*). The chain and key are read once, when the acceptor is built, which refuses a file it cannot read, a
/// file holding no certificate or no key, and a key that does not match the chain's first certificate.
#[derive(Debug, Clone)]
pub struct TlsAcceptor {
    // Shared, not copied: rustls takes its configuration as an `Arc` for every connection it accepts.
    config: Arc<ServerConfig>,
}

impl TlsAcceptor {
    /// The acceptor presenting the chain in `chain_file`, leaf first, with the private key in `key_file`, each a PEM
    /// file read once, here.
    ///
    /// # Errors
    ///
    /// [`TlsErrorEnum::CertificateUnreadable`](crate::TlsErrorEnum::CertificateUnreadable) naming a file that cannot
    /// be read, [`CertificateEmpty`](crate::TlsErrorEnum::CertificateEmpty) naming a file holding no certificate or
    /// no private key, [`KeyMismatch`](crate::TlsErrorEnum::KeyMismatch) naming the key file when the key does not
    /// match the chain's first certificate, and [`ConfigurationRefused`](crate::TlsErrorEnum::ConfigurationRefused)
    /// for a certificate or key rustls cannot use.
    pub fn from_pem_files(
        chain_file: &CertificateChainFile,
        key_file: &PrivateKeyFile,
    ) -> Result<Self, TlsError> {
        let chain_path = chain_file.path();
        let key_path = key_file.path();
        let chain_pem = read(chain_path)?;
        let key_pem = read(key_path)?;
        // A block that does not decode is no certificate, as one of another kind is not.
        let chain: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(&chain_pem)
            .filter_map(Result::ok)
            .collect();
        if chain.is_empty() {
            return Err(TlsError::certificate_empty(chain_path.to_string()));
        }
        let key = PrivateKeyDer::from_pem_slice(&key_pem)
            .map_err(|_| TlsError::certificate_empty(key_path.to_string()))?;
        Self::from_der(chain, key, key_path)
    }

    /// The acceptor presenting `chain`, leaf first, with `key`; `key_source` names the key in a refusal.
    ///
    /// # Errors
    ///
    /// [`TlsErrorEnum::CertificateEmpty`](crate::TlsErrorEnum::CertificateEmpty) naming `key_source` for an empty
    /// chain, [`KeyMismatch`](crate::TlsErrorEnum::KeyMismatch) naming it when the key does not match the chain's
    /// first certificate, and [`ConfigurationRefused`](crate::TlsErrorEnum::ConfigurationRefused) for a certificate
    /// or key rustls cannot use.
    pub fn from_der(
        chain: Vec<CertificateDer<'static>>,
        key: PrivateKeyDer<'static>,
        key_source: &str,
    ) -> Result<Self, TlsError> {
        if chain.is_empty() {
            return Err(TlsError::certificate_empty(key_source.to_string()));
        }
        let refused =
            |e: rustls::Error| TlsError(TlsErrorEnum::ConfigurationRefused(e.to_string()));
        // TLS 1.3 alone: a client offering 1.2 is refused at its hello, with no downgrade.
        let config =
            ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_protocol_versions(&[&rustls::version::TLS13])
                .map_err(refused)?
                .with_no_client_auth()
                .with_single_cert(chain, key)
                .map_err(|e| {
                    if matches!(
                        e,
                        rustls::Error::InconsistentKeys(InconsistentKeys::KeyMismatch)
                    ) {
                        TlsError::key_mismatch(key_source.to_string())
                    } else {
                        refused(e)
                    }
                })?;
        Ok(Self {
            config: Arc::new(config),
        })
    }

    /// A server session, for a carrier that drives it.
    ///
    /// # Errors
    ///
    /// [`TlsErrorEnum::ConfigurationRefused`](crate::TlsErrorEnum::ConfigurationRefused) if rustls cannot open a
    /// connection of the configuration.
    pub fn session(&self) -> Result<TlsSession, TlsError> {
        let server = ServerConnection::new(Arc::clone(&self.config))
            .map_err(|e| TlsError(TlsErrorEnum::ConfigurationRefused(e.to_string())))?;
        Ok(TlsSession::new(Connection::Server(server), String::new()))
    }

    /// Wraps `stream`, a socket the caller accepted or any other, in a TLS connection, with the handshake completed
    /// here, before the caller reads or writes a byte. A socket's read and write timeouts bound the handshake.
    ///
    /// # Errors
    ///
    /// [`TlsErrorEnum::HandshakeFailed`](crate::TlsErrorEnum::HandshakeFailed), its host empty, for a handshake that
    /// did not complete, such as a client offering no TLS 1.3, and as [`Self::session`].
    pub fn accept<S: Read + Write>(&self, stream: S) -> Result<TlsStream<S>, TlsError> {
        TlsStream::handshake(self.session()?, stream)
    }
}

/// The PEM file at `path`, read once.
fn read(path: &str) -> Result<Vec<u8>, TlsError> {
    std::fs::read(path)
        .map_err(|e| TlsError::certificate_unreadable(path.to_string(), e.to_string()))
}
