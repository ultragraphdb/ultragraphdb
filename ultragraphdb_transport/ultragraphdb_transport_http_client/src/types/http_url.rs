/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::http_client_error::{HttpClientError, HttpClientErrorEnum};

/// Where a service listens: its scheme, host and port, and nothing else, since a request names its own path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpUrl {
    text: String,
    secure: bool,
    host: String,
    port: u16,
}

impl HttpUrl {
    /// The URL `text`, `http://host:port` or `https://host:port`.
    ///
    /// # Errors
    ///
    /// [`HttpClientErrorEnum::Url`] for anything else.
    pub fn parse(text: &str) -> Result<Self, HttpClientError> {
        let refused = || {
            HttpClientError(HttpClientErrorEnum::Url {
                url: text.to_string(),
            })
        };
        let (secure, rest) = if let Some(rest) = text.strip_prefix("https://") {
            (true, rest)
        } else if let Some(rest) = text.strip_prefix("http://") {
            (false, rest)
        } else {
            return Err(refused());
        };
        let (host, port) = rest.rsplit_once(':').ok_or_else(refused)?;
        let port: u16 = port.parse().map_err(|_| refused())?;
        let plain = |byte: u8| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'[' | b']' | b':')
        };
        if host.is_empty() || !host.bytes().all(plain) {
            return Err(refused());
        }
        Ok(Self {
            text: text.to_string(),
            secure,
            host: host.to_string(),
            port,
        })
    }

    /// Whether it asks for TLS.
    #[must_use]
    pub fn is_secure(&self) -> bool {
        self.secure
    }

    /// The host.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The port.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The URL as given.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }
}
