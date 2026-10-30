/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_transport::Authenticator;

use crate::traits::credentials::Credentials;
use crate::types::scram_exchange::ScramExchange;
use crate::types::scram_server::ScramServer;
use crate::utils::constants::{DEFAULT_ITERATIONS, MECHANISM};

/// A server's authenticator of SCRAM-SHA-256 over its `credentials`: each exchange takes the next server nonce from
/// `nonces`, the caller's source of randomness, and answers unknown names with verifiers derived from `secret`.
#[derive(Debug, Clone)]
pub struct ScramAuthenticator<C, N> {
    credentials: C,
    nonces: N,
    secret: [u8; 32],
    iterations: u32,
}

impl<C, N> ScramAuthenticator<C, N> {
    pub fn new(credentials: C, nonces: N, secret: [u8; 32]) -> Self {
        Self {
            credentials,
            nonces,
            secret,
            iterations: DEFAULT_ITERATIONS,
        }
    }

    /// The authenticator answering unknown names with verifiers of `iterations`, the count its credentials hold
    /// ([`ScramServer::with_iterations`]).
    pub fn with_iterations(mut self, iterations: u32) -> Self {
        self.iterations = iterations;
        self
    }
}

impl<C: Credentials + Clone, N: FnMut() -> String> Authenticator for ScramAuthenticator<C, N> {
    type Mechanism = ScramExchange<C>;

    /// An exchange of [`crate::MECHANISM`], its server nonce the next of `nonces`; `None` for any other mechanism.
    fn start(&mut self, mechanism: &str) -> Option<ScramExchange<C>> {
        if mechanism != MECHANISM {
            return None;
        }
        let server =
            ScramServer::new(&(self.nonces)(), self.secret).with_iterations(self.iterations);
        Some(ScramExchange::new(server, self.credentials.clone()))
    }
}
