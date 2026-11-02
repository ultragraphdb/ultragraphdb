/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod plain_http_client_http_client;

use std::time::Duration;

/// The HTTP/1.1 client the OTLP exporter sends through: one request per connection over `std::net`, `http://`
/// only, so TLS is the collector's to terminate. A request, from connecting to the response's last byte, takes at
/// most the client's timeout. It blocks, as the SDK's batch span processor expects: the processor runs each export
/// on a thread of its own under `futures_executor::block_on`, so no async runtime is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlainHttpClient {
    timeout: Duration,
}

impl PlainHttpClient {
    /// The client whose requests each take at most `timeout`.
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// The bound on one request, from connecting to the response's last byte.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}
