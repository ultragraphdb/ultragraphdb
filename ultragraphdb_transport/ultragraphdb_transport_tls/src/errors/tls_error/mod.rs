/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod tls_error_display;
mod tls_error_error;

/// Why a trust store or a server's identity could not be built, or a verified connection could not be opened or
/// carried on.
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TlsError(pub TlsErrorEnum);

/// The classification of a [`TlsError`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TlsErrorEnum {
    /// The trust store file could not be read: its path and the reason.
    TrustStoreUnreadable {
        /// The file's path.
        path: String,
        /// Why it could not be read.
        reason: String,
    },
    /// The trust store holds no certificate a verifier can use: where it came from.
    TrustStoreEmpty(String),
    /// The host is no name or address a certificate can be checked against.
    InvalidServerName(String),
    /// The handshake did not complete or did not verify, such as for an untrusted, expired or misnamed certificate.
    HandshakeFailed {
        /// The host the connection was opened for.
        host: String,
        /// rustls's reason.
        reason: String,
    },
    /// rustls or rcgen refused to build a configuration, a key or a certificate: the reason.
    ConfigurationRefused(String),
    /// A server's certificate chain or key file could not be read: its path and the reason.
    CertificateUnreadable {
        /// The file's path.
        path: String,
        /// Why it could not be read.
        reason: String,
    },
    /// A server's certificate chain file holds no certificate, or its key file no private key: the file.
    CertificateEmpty(String),
    /// The private key does not match the chain's first certificate: the key's source.
    KeyMismatch(String),
    /// A record after the handshake did not decrypt or decode, or the peer's alert ended the session.
    RecordRefused {
        /// The peer the session is with: the host a client opened it for, empty on the accepting side.
        host: String,
        /// rustls's reason.
        reason: String,
    },
}

impl TlsError {
    /// [`TlsErrorEnum::TrustStoreUnreadable`] of `path` for `reason`.
    #[must_use]
    pub fn trust_store_unreadable(path: String, reason: String) -> Self {
        Self(TlsErrorEnum::TrustStoreUnreadable { path, reason })
    }

    /// [`TlsErrorEnum::TrustStoreEmpty`] of `source`.
    #[must_use]
    pub fn trust_store_empty(source: String) -> Self {
        Self(TlsErrorEnum::TrustStoreEmpty(source))
    }

    /// [`TlsErrorEnum::InvalidServerName`] of `host`.
    #[must_use]
    pub fn invalid_server_name(host: String) -> Self {
        Self(TlsErrorEnum::InvalidServerName(host))
    }

    /// [`TlsErrorEnum::HandshakeFailed`] with `host` for `reason`.
    #[must_use]
    pub fn handshake_failed(host: String, reason: String) -> Self {
        Self(TlsErrorEnum::HandshakeFailed { host, reason })
    }

    /// [`TlsErrorEnum::CertificateUnreadable`] of `path` for `reason`.
    #[must_use]
    pub fn certificate_unreadable(path: String, reason: String) -> Self {
        Self(TlsErrorEnum::CertificateUnreadable { path, reason })
    }

    /// [`TlsErrorEnum::CertificateEmpty`] of `path`.
    #[must_use]
    pub fn certificate_empty(path: String) -> Self {
        Self(TlsErrorEnum::CertificateEmpty(path))
    }

    /// [`TlsErrorEnum::KeyMismatch`] of the key from `source`.
    #[must_use]
    pub fn key_mismatch(source: String) -> Self {
        Self(TlsErrorEnum::KeyMismatch(source))
    }

    /// [`TlsErrorEnum::RecordRefused`] with `host` for `reason`.
    #[must_use]
    pub fn record_refused(host: String, reason: String) -> Self {
        Self(TlsErrorEnum::RecordRefused { host, reason })
    }
}
