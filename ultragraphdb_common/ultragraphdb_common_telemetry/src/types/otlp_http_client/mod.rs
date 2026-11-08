/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod otlp_http_client_http_client;

use std::time::Duration;

use ultragraphdb_transport_tls::TlsConnector;

use crate::traits::dial::Dial;
use crate::types::tcp_dial::TcpDial;

/// The HTTP/1.1 client the OTLP exporter sends through: one request per connection its [`Dial`] opens, TCP unless
/// named otherwise, to an `http://` endpoint, or to an `https://` endpoint verified by its [`TlsConnector`]. Dialing
/// and every read and write each wait at most the client's timeout. It blocks, as the SDK's batch span processor
/// expects: the processor runs each export on a thread of its own under `futures_executor::block_on`, so no async
/// runtime is needed.
#[derive(Debug, Clone)]
pub struct OtlpHttpClient<D = TcpDial> {
    dial: D,
    timeout: Duration,
    tls: Option<TlsConnector>,
}

impl OtlpHttpClient {
    /// The client over TCP whose every step waits at most `timeout`, to `http://` endpoints only.
    #[must_use]
    pub fn new(timeout: Duration) -> Self {
        Self::over(TcpDial, timeout)
    }
}

impl<D: Dial> OtlpHttpClient<D> {
    /// The client over the connections `dial` opens, whose every step waits at most `timeout`, to `http://`
    /// endpoints only.
    pub fn over(dial: D, timeout: Duration) -> Self {
        Self {
            dial,
            timeout,
            tls: None,
        }
    }

    /// The client sending to `https://` endpoints too, verified through `connector`.
    #[must_use]
    pub fn with_tls(self, connector: TlsConnector) -> Self {
        Self {
            tls: Some(connector),
            ..self
        }
    }

    /// What opens the client's connections.
    pub fn dial(&self) -> &D {
        &self.dial
    }

    /// The bound on each step of a request: dialing, and every read and write.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// The connector `https://` requests are verified through, if any.
    pub fn tls(&self) -> Option<&TlsConnector> {
        self.tls.as_ref()
    }
}
