/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use super::{TlsError, TlsErrorEnum};

impl fmt::Display for TlsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            TlsErrorEnum::TrustStoreUnreadable { path, reason } => {
                write!(f, "cannot read the trust store {path:?}: {reason}")
            }
            TlsErrorEnum::TrustStoreEmpty(source) => {
                write!(f, "no usable certificate in {source}")
            }
            TlsErrorEnum::InvalidServerName(host) => {
                write!(f, "no certificate can be checked against {host:?}")
            }
            TlsErrorEnum::HandshakeFailed { host, reason } => {
                write!(f, "TLS handshake with {host:?} failed: {reason}")
            }
            TlsErrorEnum::ConfigurationRefused(reason) => {
                write!(f, "TLS configuration refused: {reason}")
            }
            TlsErrorEnum::CertificateUnreadable { path, reason } => {
                write!(
                    f,
                    "cannot read the server certificate file {path:?}: {reason}"
                )
            }
            TlsErrorEnum::CertificateEmpty(path) => {
                write!(f, "no certificate or private key in {path}")
            }
            TlsErrorEnum::KeyMismatch(source) => write!(
                f,
                "the private key in {source} does not match the chain's first certificate"
            ),
            TlsErrorEnum::RecordRefused { host, reason } => {
                write!(f, "TLS session with {host:?} failed: {reason}")
            }
        }
    }
}
