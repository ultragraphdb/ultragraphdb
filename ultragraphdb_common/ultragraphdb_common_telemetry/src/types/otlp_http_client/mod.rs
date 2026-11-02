/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod otlp_http_client_http_client;

use std::time::Duration;

use ultragraphdb_transport_tls::TlsConnector;

/// The HTTP/1.1 client the OTLP exporter sends through: one request per connection over `std::net`, to an
/// `http://` endpoint, or to an `https://` endpoint verified by its [`TlsConnector`]. A request, from connecting to
/// the response's last byte, takes at most the client's timeout. It blocks, as the SDK's batch span processor
/// expects: the processor runs each export on a thread of its own under `futures_executor::block_on`, so no async
/// runtime is needed.
#[derive(Debug, Clone)]
pub struct OtlpHttpClient {
    timeout: Duration,
    tls: Option<TlsConnector>,
}

impl OtlpHttpClient {
    /// The client whose requests each take at most `timeout`, to `http://` endpoints only.
    pub fn new(timeout: Duration) -> Self {
        Self { timeout, tls: None }
    }

    /// The client sending to `https://` endpoints too, verified through `connector`.
    pub fn with_tls(self, connector: TlsConnector) -> Self {
        Self {
            tls: Some(connector),
            ..self
        }
    }

    /// The bound on one request, from connecting to the response's last byte.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// The connector `https://` requests are verified through, if any.
    pub fn tls(&self) -> Option<&TlsConnector> {
        self.tls.as_ref()
    }
}
