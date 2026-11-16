/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod scram_server_debug;

use crate::errors::scram_error::ScramError;
use crate::types::scram_relay::{ScramRelay, read_name};
use crate::types::scram_verifier::ScramVerifier;
use crate::utils::constants::DEFAULT_ITERATIONS;

/// The server's side of SCRAM-SHA-256 (RFC 5802 §5): the server-first-message with the salt and iteration count of
/// the name's verifier, the check of the client's proof in constant time, and the server-final-message with its
/// signature. A name no principal holds takes a verifier derived from the server's `secret`, the same for the same
/// name, so its exchange runs to the proof and fails there as a wrong password's does. It runs the two halves in one
/// process: a [`ScramRelay`] for the messages, and [`ScramVerifier::sign`] over the verifier it holds between them. Its
/// `Debug` holds the iteration count, the relay and the principal held with its verifier, never the secret, a key, or
/// the text of a message or nonce, which the relay prints as its length.
#[derive(Clone, PartialEq, Eq)]
pub struct ScramServer {
    secret: [u8; 32],
    iterations: u32,
    relay: ScramRelay,
    /// From the server-first-message on: the principal holding the name, if one does, and its verifier.
    held: Option<(Option<u64>, ScramVerifier)>,
}

impl ScramServer {
    /// An exchange whose server nonce part is `nonce`, printable ASCII without a comma, and whose unknown names take
    /// verifiers derived from `secret`.
    #[must_use]
    pub fn new(nonce: &str, secret: [u8; 32]) -> Self {
        Self {
            secret,
            iterations: DEFAULT_ITERATIONS,
            relay: ScramRelay::new(nonce),
            held: None,
        }
    }

    /// The exchange with the iteration count an unknown name's verifier takes: the count the server derives its
    /// verifiers with, so an unknown name costs a client what a known one does. [`crate::DEFAULT_ITERATIONS`] by
    /// default.
    #[must_use]
    pub fn with_iterations(mut self, iterations: u32) -> Self {
        self.iterations = iterations;
        self
    }

    /// The SASLprep-prepared name `client_first` asks for, for a server that fetches the name's verifier before it runs
    /// the exchange.
    ///
    /// # Errors
    ///
    /// As [`ScramServer::first`] refuses the message's header and name: [`ScramError::malformed`] for a header or name
    /// the message does not hold in its form, [`ScramError::channel_binding`] for a client asking for channel binding,
    /// [`ScramError::prepare`] for a name SASLprep refuses.
    pub fn name_in(client_first: &[u8]) -> Result<String, ScramError> {
        read_name(client_first).map(|(_, _, name)| name)
    }

    /// The server-first-message answering `client_first`, the name's verifier found by `lookup`: `r=<client nonce +
    /// server nonce>,s=<salt>,i=<iterations>`.
    ///
    /// # Errors
    ///
    /// [`ScramError::malformed`], [`ScramError::channel_binding`] for a client asking for channel binding,
    /// [`ScramError::prepare`], or [`ScramError::out_of_step`].
    pub fn first(
        &mut self,
        client_first: &[u8],
        lookup: impl FnOnce(&str) -> Option<(u64, ScramVerifier)>,
    ) -> Result<Vec<u8>, ScramError> {
        let name = self.relay.first(client_first)?;
        let (principal, verifier) = match lookup(&name) {
            Some((principal, verifier)) => (Some(principal), verifier),
            None => (
                None,
                ScramVerifier::unknown(&self.secret, &name, self.iterations),
            ),
        };
        let server_first = self
            .relay
            .challenge(verifier.salt(), verifier.iterations())?;
        self.held = Some((principal, verifier));
        Ok(server_first)
    }

    /// The server-final-message answering `client_final`, `v=<signature>`, and the principal it authenticated.
    ///
    /// # Errors
    ///
    /// [`ScramError::proof`] for a wrong password or an unknown name alike, [`ScramError::nonce`] for another nonce,
    /// [`ScramError::channel_binding`] for another channel binding than the first message's, [`ScramError::malformed`],
    /// or [`ScramError::out_of_step`].
    pub fn last(&mut self, client_final: &[u8]) -> Result<(Vec<u8>, u64), ScramError> {
        let Some((principal, verifier)) = &self.held else {
            return Err(ScramError::out_of_step());
        };
        let (auth_message, proof) = self.relay.last(client_final)?;
        let (Some(signature), Some(principal)) = (verifier.sign(&auth_message, &proof), *principal)
        else {
            return Err(ScramError::proof());
        };
        Ok((self.relay.finish(&signature)?, principal))
    }
}
