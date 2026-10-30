/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod scram_exchange_server_mechanism;

use crate::types::scram_server::ScramServer;

/// One SCRAM-SHA-256 exchange of a server over its credentials, the transport's server mechanism: the client's first
/// message looked up in them, its final message proved. A password's credential never expires.
#[derive(Debug, Clone)]
pub struct ScramExchange<C> {
    server: ScramServer,
    credentials: C,
    started: bool,
}

impl<C> ScramExchange<C> {
    /// The exchange of `server` over `credentials`.
    pub fn new(server: ScramServer, credentials: C) -> Self {
        Self {
            server,
            credentials,
            started: false,
        }
    }
}
