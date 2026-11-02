/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

use rustls::{ClientConfig, ClientConnection, StreamOwned};
use rustls_pki_types::ServerName;

use crate::errors::tls_error::TlsError;
use crate::types::tls_trust::TlsTrust;

/// A verified connection over `S`, a socket unless named otherwise, read and written as any other stream; the
/// stream beneath is `.sock`.
pub type TlsStream<S = TcpStream> = StreamOwned<ClientConnection, S>;

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
    #[must_use]
    pub fn new(trust: TlsTrust) -> Self {
        debug_assert!(!trust.is_empty());
        let config =
            ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
                .expect("ring's provider supports TLS 1.3 and 1.2")
                .with_root_certificates(trust.roots())
                .with_no_client_auth();
        Self {
            config: Arc::new(config),
            trust,
        }
    }

    /// The certificates this verifies against.
    #[must_use]
    pub fn trust(&self) -> &TlsTrust {
        &self.trust
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
        let name = ServerName::try_from(host.to_string())
            .map_err(|_| TlsError::InvalidServerName(host.to_string()))?;
        let failed = |reason: String| TlsError::HandshakeFailed(host.to_string(), reason);
        let connection = ClientConnection::new(Arc::clone(&self.config), name)
            .map_err(|e| failed(e.to_string()))?;
        let mut tls = StreamOwned::new(connection, stream);
        debug_assert!(tls.conn.is_handshaking());
        // rustls defers the handshake to the first read or write, where a refused certificate would surface as an
        // I/O error mid-request. Called while handshaking, `complete_io` writes and reads until the handshake
        // completes or fails, so one call settles it.
        let (_read_bytes, _written_bytes) = tls
            .conn
            .complete_io(&mut tls.sock)
            .map_err(|e| failed(e.to_string()))?;
        debug_assert!(!tls.conn.is_handshaking());
        Ok(tls)
    }
}
