/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod tls_error_display;
mod tls_error_error;

/// Why a trust store could not be built or a verified connection could not be opened.
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
}

impl TlsError {
    /// [`TlsErrorEnum::TrustStoreUnreadable`] of `path` for `reason`.
    #[expect(non_snake_case, reason = "a constructor carries its variant's name")]
    pub fn TrustStoreUnreadable(path: String, reason: String) -> Self {
        Self(TlsErrorEnum::TrustStoreUnreadable { path, reason })
    }

    /// [`TlsErrorEnum::TrustStoreEmpty`] of `source`.
    #[expect(non_snake_case, reason = "a constructor carries its variant's name")]
    pub fn TrustStoreEmpty(source: String) -> Self {
        Self(TlsErrorEnum::TrustStoreEmpty(source))
    }

    /// [`TlsErrorEnum::InvalidServerName`] of `host`.
    #[expect(non_snake_case, reason = "a constructor carries its variant's name")]
    pub fn InvalidServerName(host: String) -> Self {
        Self(TlsErrorEnum::InvalidServerName(host))
    }

    /// [`TlsErrorEnum::HandshakeFailed`] with `host` for `reason`.
    #[expect(non_snake_case, reason = "a constructor carries its variant's name")]
    pub fn HandshakeFailed(host: String, reason: String) -> Self {
        Self(TlsErrorEnum::HandshakeFailed { host, reason })
    }
}
