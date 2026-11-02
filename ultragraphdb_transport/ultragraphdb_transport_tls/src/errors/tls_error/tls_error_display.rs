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
        }
    }
}
