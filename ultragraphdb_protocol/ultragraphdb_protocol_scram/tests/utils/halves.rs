/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The two halves of a server's exchange driven as compute and the system group drive them across Challenge and
//! Verify (design D8): [`ScramRelay`] reads and builds the messages; the group finds the name's verifier, or derives
//! an unknown name's from its secret, and signs a proof that holds; a Verify answered `verified` 0 is refused. Its
//! answers take [`ultragraphdb_protocol_scram::ScramServer`]'s shape, so a test compares the two exchanges message for
//! message.

use ultragraphdb_protocol_scram::{ScramError, ScramRelay, ScramVerifier};

/// A Challenge's answer: the principal holding the name, if one does, and the verifier the group checks against.
type Held = (Option<u64>, ScramVerifier);

/// One exchange through the two halves, the group holding the principals `lookup` finds and the `secret` an unknown
/// name's verifier derives from at `iterations`.
pub struct Halves<L> {
    relay: ScramRelay,
    lookup: L,
    secret: [u8; 32],
    iterations: u32,
    held: Option<Held>,
}

impl<L: Fn(&str) -> Option<(u64, ScramVerifier)>> Halves<L> {
    /// The exchange of the relay's server nonce `nonce`.
    pub fn new(nonce: &str, secret: [u8; 32], iterations: u32, lookup: L) -> Self {
        Self {
            relay: ScramRelay::new(nonce),
            lookup,
            secret,
            iterations,
            held: None,
        }
    }

    /// The group's answer to a Challenge for `name`: the name's principal and verifier, or an unknown name's verifier.
    pub fn challenge(&self, name: &str) -> Held {
        match (self.lookup)(name) {
            Some((principal, verifier)) => (Some(principal), verifier),
            None => (
                None,
                ScramVerifier::unknown(&self.secret, name, self.iterations),
            ),
        }
    }

    /// The server-first-message answering `client_first`, as `ScramServer::first` answers it.
    pub fn first(&mut self, client_first: &[u8]) -> Result<Vec<u8>, ScramError> {
        let name = self.relay.first(client_first)?;
        let (principal, verifier) = self.challenge(&name);
        let server_first = self
            .relay
            .challenge(verifier.salt(), verifier.iterations())?;
        self.held = Some((principal, verifier));
        Ok(server_first)
    }

    /// The server-final-message answering `client_final` and the principal, as `ScramServer::last` answers them: a
    /// proof the group does not sign, or a name no principal holds, is refused as [`ScramError::proof`].
    pub fn last(&mut self, client_final: &[u8]) -> Result<(Vec<u8>, u64), ScramError> {
        let (auth_message, proof) = self.relay.last(client_final)?;
        let Some((principal, verifier)) = &self.held else {
            return Err(ScramError::out_of_step());
        };
        match (verifier.sign(&auth_message, &proof), principal) {
            (Some(signature), Some(principal)) => Ok((self.relay.finish(&signature)?, *principal)),
            _ => Err(ScramError::proof()),
        }
    }

    /// The relay, as the endpoint holds it.
    pub fn relay(&self) -> &ScramRelay {
        &self.relay
    }
}
