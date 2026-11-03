/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::{Read, Write};
use std::sync::Arc;

use rustls::{ClientConfig, ClientConnection, Connection};
use rustls_pki_types::ServerName;

use crate::errors::tls_error::{TlsError, TlsErrorEnum};
use crate::types::tls_session::TlsSession;
use crate::types::tls_stream::TlsStream;
use crate::types::tls_trust::TlsTrust;

/// Opens verified connections against one [`TlsTrust`]: TLS 1.3 and 1.2, ring as the crypto provider, no client
/// certificate. It wraps a stream the caller opened, a socket or any other, for the name the caller asked for, which
/// is what the certificate is checked against.
#[derive(Debug, Clone)]
pub struct TlsConnector {
    // Shared, not copied: rustls takes its configuration as an `Arc` for every connection it opens.
    config: Arc<ClientConfig>,
    trust: TlsTrust,
}

impl TlsConnector {
    /// The connector verifying against `trust`. The provider and the versions are named here rather than taken
    /// from the process default, so a second provider compiled into the binary later cannot make the choice
    /// ambiguous; TLS before 1.2 stays off.
    ///
    /// # Errors
    ///
    /// [`TlsErrorEnum::ConfigurationRefused`](crate::TlsErrorEnum::ConfigurationRefused) if rustls refuses the
    /// provider's protocol versions, which ring's provider supports both of.
    pub fn new(trust: TlsTrust) -> Result<Self, TlsError> {
        debug_assert!(!trust.is_empty());
        let config =
            ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
                .map_err(|e| TlsError(TlsErrorEnum::ConfigurationRefused(e.to_string())))?
                .with_root_certificates(trust.roots())
                .with_no_client_auth();
        Ok(Self {
            config: Arc::new(config),
            trust,
        })
    }

    /// The certificates this verifies against.
    #[must_use]
    pub fn trust(&self) -> &TlsTrust {
        &self.trust
    }

    /// A client session verifying the server for `host`, a DNS name or an IP address, for a carrier that drives it.
    ///
    /// # Errors
    ///
    /// [`TlsErrorEnum::InvalidServerName`](crate::TlsErrorEnum::InvalidServerName) for a host no certificate can
    /// be checked against, and [`HandshakeFailed`](crate::TlsErrorEnum::HandshakeFailed) if rustls cannot open the
    /// connection.
    pub fn session(&self, host: &str) -> Result<TlsSession, TlsError> {
        let name = ServerName::try_from(host.to_string())
            .map_err(|_| TlsError::invalid_server_name(host.to_string()))?;
        let client = ClientConnection::new(Arc::clone(&self.config), name)
            .map_err(|e| TlsError::handshake_failed(host.to_string(), e.to_string()))?;
        Ok(TlsSession::new(
            Connection::Client(client),
            host.to_string(),
        ))
    }

    /// Wraps `stream` in a connection verified for `host`, a DNS name or an IP address, with the handshake
    /// completed here: a certificate that does not verify fails this call, before anything is sent. A socket's read
    /// and write timeouts bound the handshake.
    ///
    /// # Errors
    ///
    /// [`TlsErrorEnum::InvalidServerName`](crate::TlsErrorEnum::InvalidServerName) for a host no certificate can
    /// be checked against, and [`HandshakeFailed`](crate::TlsErrorEnum::HandshakeFailed) for a handshake that did
    /// not complete or a certificate that is untrusted, expired or issued for another name.
    pub fn connect<S: Read + Write>(
        &self,
        stream: S,
        host: &str,
    ) -> Result<TlsStream<S>, TlsError> {
        TlsStream::handshake(self.session(host)?, stream)
    }
}
