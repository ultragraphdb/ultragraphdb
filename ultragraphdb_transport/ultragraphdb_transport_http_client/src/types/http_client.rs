/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use compio::BufResult;
use compio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_transport_compio::{Carrier, CompioTlsStream};
use ultragraphdb_transport_tls::{TlsConnector, TlsTrust};

use crate::errors::http_client_error::{HttpClientError, HttpClientErrorEnum};
use crate::types::http_reply::{HttpReply, ParsedReply};
use crate::types::http_url::HttpUrl;
use crate::utils::parse_reply::parse_reply;

/// The bytes one read asks for.
const READ_BYTES: usize = 16 * 1024;

/// The most a reply's body holds.
const BODY_MAX: usize = 1 << 20;

/// How long one exchange takes at most, its dial and its TLS handshake included.
const EXCHANGE_TIME: Duration = Duration::from_secs(5);

/// A client of one service, holding its URL, its trust and the key it presents. Its `Debug` holds no key.
#[derive(Clone)]
pub struct HttpClient {
    url: HttpUrl,
    connector: Option<TlsConnector>,
    key: String,
}

impl std::fmt::Debug for HttpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpClient")
            .field("url", &self.url.as_str())
            .finish_non_exhaustive()
    }
}

impl HttpClient {
    /// A client of `url`, presenting `key`, verifying an `https` service against `trust`.
    ///
    /// # Errors
    ///
    /// [`HttpClientErrorEnum::Trust`] for an `https` URL with no trust, or a trust that gives no connector.
    pub fn new(
        url: HttpUrl,
        trust: Option<TlsTrust>,
        key: String,
    ) -> Result<Self, HttpClientError> {
        let refused = |reason: String| HttpClientError(HttpClientErrorEnum::Trust { reason });
        let connector = if url.is_secure() {
            let trust = trust.ok_or_else(|| {
                refused(format!(
                    "{} asks for TLS, and no trust is named",
                    url.as_str()
                ))
            })?;
            Some(TlsConnector::new(trust).map_err(|error| refused(error.to_string()))?)
        } else {
            None
        };
        Ok(Self {
            url,
            connector,
            key,
        })
    }

    /// The service's URL.
    #[must_use]
    pub fn url(&self) -> &HttpUrl {
        &self.url
    }

    /// Sends `method` of `target` with `body`, and reads the whole reply.
    ///
    /// # Errors
    ///
    /// [`HttpClientErrorEnum::Unreachable`] and [`HttpClientErrorEnum::TimedOut`] for what waiting may mend;
    /// [`HttpClientErrorEnum::Tls`] and [`HttpClientErrorEnum::Malformed`] for the rest. A status is a reply, whatever it is.
    pub async fn request(
        &self,
        method: &str,
        target: &str,
        body: &[u8],
    ) -> Result<HttpReply, HttpClientError> {
        let exchange = self.exchange(method, target, body);
        match compio::time::timeout(EXCHANGE_TIME, exchange).await {
            Ok(reply) => reply,
            Err(_) => Err(HttpClientError(HttpClientErrorEnum::TimedOut {
                url: self.url.as_str().to_string(),
            })),
        }
    }

    async fn exchange(
        &self,
        method: &str,
        target: &str,
        body: &[u8],
    ) -> Result<HttpReply, HttpClientError> {
        let text = self.url.as_str().to_string();
        let unreachable = |reason: String| {
            HttpClientError(HttpClientErrorEnum::Unreachable {
                url: text.clone(),
                reason,
            })
        };
        let address = Address::parse(&format!("tcp://{}:{}", self.url.host(), self.url.port()))
            .map_err(|error| unreachable(error.to_string()))?;
        let carrier = Carrier::connect(&address)
            .await
            .map_err(|error| unreachable(error.to_string()))?;
        let Some(connector) = &self.connector else {
            return self.over(carrier, method, target, body).await;
        };
        let stream = CompioTlsStream::connect(connector, carrier, self.url.host())
            .await
            .map_err(|error| {
                HttpClientError(HttpClientErrorEnum::Tls {
                    url: text.clone(),
                    reason: error.to_string(),
                })
            })?;
        self.over(stream, method, target, body).await
    }

    /// The exchange over `stream`.
    async fn over<S: AsyncRead + AsyncWrite>(
        &self,
        mut stream: S,
        method: &str,
        target: &str,
        body: &[u8],
    ) -> Result<HttpReply, HttpClientError> {
        let url = self.url.as_str();
        let unreachable = |reason: String| {
            HttpClientError(HttpClientErrorEnum::Unreachable {
                url: url.to_string(),
                reason,
            })
        };
        let mut message = format!(
            "{method} {target} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.url.host(),
            self.key,
            body.len()
        )
        .into_bytes();
        message.extend_from_slice(body);
        let BufResult(written, _) = stream.write_all(message).await;
        // A server may answer before it has read what it was sent, and close, which fails this write with its answer waiting
        // behind the failure. So the failure decides only when no reply follows.
        let failed = match written {
            Ok(_) => stream.flush().await.err(),
            Err(error) => Some(error),
        };
        let mut received = Vec::with_capacity(READ_BYTES);
        loop {
            match parse_reply(&received, BODY_MAX) {
                ParsedReply::Complete(reply) => return Ok(reply),
                ParsedReply::Malformed(reason) => {
                    return Err(HttpClientError(HttpClientErrorEnum::Malformed {
                        url: url.to_string(),
                        reason,
                    }));
                }
                ParsedReply::Incomplete => {}
            }
            let chunk = Vec::with_capacity(READ_BYTES);
            let BufResult(read, chunk) = stream.read(chunk).await;
            match read {
                Ok(count) if count > 0 => received.extend_from_slice(&chunk),
                Ok(_) => {
                    return Err(failed.map_or_else(
                        || {
                            HttpClientError(HttpClientErrorEnum::Malformed {
                                url: url.to_string(),
                                reason: "the reply ended early".to_string(),
                            })
                        },
                        |error| unreachable(error.to_string()),
                    ));
                }
                Err(error) => {
                    return Err(unreachable(failed.unwrap_or(error).to_string()));
                }
            }
        }
    }
}
