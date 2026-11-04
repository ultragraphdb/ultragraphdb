/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

/// Why a request got no reply. The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpClientErrorEnum {
    /// The URL is not `http://host:port` or `https://host:port`.
    Url {
        /// The URL.
        url: String,
    },
    /// An `https` URL with no trust to verify it, or a trust that does not load.
    Trust {
        /// Why not.
        reason: String,
    },
    /// A key's file that cannot be read, or holds no key.
    Key {
        /// The file.
        file: String,
        /// Why not.
        reason: String,
    },
    /// Nothing answers at the URL.
    Unreachable {
        /// The URL.
        url: String,
        /// The failure.
        reason: String,
    },
    /// The peer did not answer in time.
    TimedOut {
        /// The URL.
        url: String,
    },
    /// The TLS handshake did not complete or did not verify.
    Tls {
        /// The URL.
        url: String,
        /// Why not.
        reason: String,
    },
    /// The peer answered with something that is no HTTP response, or more than a reply holds.
    Malformed {
        /// The URL.
        url: String,
        /// Why not.
        reason: String,
    },
}

/// Why a request got no reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpClientError(pub HttpClientErrorEnum);

impl HttpClientError {
    /// Whether waiting may mend this: nothing answered, or not in time.
    #[must_use]
    pub fn is_unreachable(&self) -> bool {
        matches!(
            self.0,
            HttpClientErrorEnum::Unreachable { .. } | HttpClientErrorEnum::TimedOut { .. }
        )
    }
}

impl fmt::Display for HttpClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            HttpClientErrorEnum::Url { url } => {
                write!(
                    f,
                    "the URL {url} is not http://host:port or https://host:port"
                )
            }
            HttpClientErrorEnum::Trust { reason } => write!(f, "the TLS trust: {reason}"),
            HttpClientErrorEnum::Key { file, reason } => write!(f, "the key file {file}: {reason}"),
            HttpClientErrorEnum::Unreachable { url, reason } => {
                write!(f, "nothing answers at {url}: {reason}")
            }
            HttpClientErrorEnum::TimedOut { url } => write!(f, "{url} did not answer in time"),
            HttpClientErrorEnum::Tls { url, reason } => write!(f, "TLS with {url}: {reason}"),
            HttpClientErrorEnum::Malformed { url, reason } => {
                write!(f, "{url} answered with no HTTP response: {reason}")
            }
        }
    }
}

impl std::error::Error for HttpClientError {}
