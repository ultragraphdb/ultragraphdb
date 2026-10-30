/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::scram_error::ScramError;
use crate::types::scram_verifier::ScramVerifier;
use crate::utils::constants::{DEFAULT_ITERATIONS, SALT_SIZE};
use crate::utils::crypto::{hmac, prepare, same, sha256};
use crate::utils::message::{attribute, decode, encode, unescape, valid_nonce};

/// Where a server's exchange stands.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ServerState {
    Initial,
    First {
        binding: String,
        client_first_bare: String,
        server_first: String,
        nonce: String,
        principal: Option<u64>,
        verifier: Box<ScramVerifier>,
    },
    Done,
}

/// The server's side of SCRAM-SHA-256 (RFC 5802 §5): the server-first-message with the salt and iteration count of
/// the name's verifier, the check of the client's proof in constant time, and the server-final-message with its
/// signature. A name no principal holds takes a verifier derived from the server's `secret`, the same for the same
/// name, so its exchange runs to the proof and fails there as a wrong password's does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScramServer {
    nonce: String,
    secret: [u8; 32],
    iterations: u32,
    state: ServerState,
}

impl ScramServer {
    /// An exchange whose server nonce part is `nonce`, printable ASCII without a comma, and whose unknown names take
    /// verifiers derived from `secret`.
    pub fn new(nonce: &str, secret: [u8; 32]) -> Self {
        Self {
            nonce: nonce.to_string(),
            secret,
            iterations: DEFAULT_ITERATIONS,
            state: ServerState::Initial,
        }
    }

    /// The exchange with the iteration count an unknown name's verifier takes: the count the server derives its
    /// verifiers with, so an unknown name costs a client what a known one does. [`crate::DEFAULT_ITERATIONS`] by
    /// default.
    pub fn with_iterations(mut self, iterations: u32) -> Self {
        self.iterations = iterations;
        self
    }

    /// The server-first-message answering `client_first`, the name's verifier found by `lookup`: `r=<client nonce +
    /// server nonce>,s=<salt>,i=<iterations>`. Refused as [`ScramError::Malformed`], [`ScramError::ChannelBinding`]
    /// for a client asking for channel binding, [`ScramError::Prepare`], or [`ScramError::OutOfStep`].
    pub fn first(
        &mut self,
        client_first: &[u8],
        lookup: impl FnOnce(&str) -> Option<(u64, ScramVerifier)>,
    ) -> Result<Vec<u8>, ScramError> {
        if self.state != ServerState::Initial {
            return Err(ScramError::OutOfStep());
        }
        let text = std::str::from_utf8(client_first)
            .map_err(|_| ScramError::Malformed("channel binding"))?;
        let mut header = text.splitn(3, ',');
        let (flag, authorization, bare) = match (header.next(), header.next(), header.next()) {
            (Some(flag), Some(authorization), Some(bare)) => (flag, authorization, bare),
            _ => return Err(ScramError::Malformed("channel binding")),
        };
        match flag {
            "n" | "y" => {}
            f if f.starts_with("p=") => return Err(ScramError::ChannelBinding()),
            _ => return Err(ScramError::Malformed("channel binding")),
        }
        if !authorization.is_empty() {
            return Err(ScramError::Malformed("authorization identity"));
        }
        let mut parts = bare.split(',');
        let name = attribute(parts.next(), 'n').ok_or(ScramError::Malformed("name"))?;
        let name = prepare(&unescape(name)?)?;
        let client_nonce = attribute(parts.next(), 'r')
            .filter(|n| valid_nonce(n))
            .ok_or(ScramError::Malformed("nonce"))?;
        let (principal, verifier) = match lookup(&name) {
            Some((principal, verifier)) => (Some(principal), verifier),
            None => (None, self.unknown(&name)),
        };
        let nonce = format!("{client_nonce}{}", self.nonce);
        let server_first = format!(
            "r={nonce},s={},i={}",
            encode(verifier.salt()),
            verifier.iterations()
        );
        self.state = ServerState::First {
            binding: encode(format!("{flag},,").as_bytes()),
            client_first_bare: bare.to_string(),
            server_first: server_first.clone(),
            nonce,
            principal,
            verifier: Box::new(verifier),
        };
        Ok(server_first.into_bytes())
    }

    /// The verifier of a name no principal holds, derived from the server's secret: the same for the same name, its
    /// keys verifying no proof a client computes without the secret.
    fn unknown(&self, name: &str) -> ScramVerifier {
        let mut salt = hmac(&self.secret, format!("salt:{name}").as_bytes()).to_vec();
        salt.truncate(SALT_SIZE);
        ScramVerifier::new(
            salt,
            self.iterations,
            hmac(&self.secret, format!("stored:{name}").as_bytes()),
            hmac(&self.secret, format!("server:{name}").as_bytes()),
        )
    }

    /// The server-final-message answering `client_final`, `v=<signature>`, and the principal it authenticated;
    /// refused as [`ScramError::Proof`] for a wrong password or an unknown name alike, [`ScramError::Nonce`] for
    /// another nonce, [`ScramError::Malformed`], or [`ScramError::OutOfStep`].
    pub fn last(&mut self, client_final: &[u8]) -> Result<(Vec<u8>, u64), ScramError> {
        let ServerState::First {
            binding,
            client_first_bare,
            server_first,
            nonce,
            principal,
            verifier,
        } = &self.state
        else {
            return Err(ScramError::OutOfStep());
        };
        let text = std::str::from_utf8(client_final).map_err(|_| ScramError::Malformed("proof"))?;
        let (without_proof, proof) = text
            .rsplit_once(",p=")
            .ok_or(ScramError::Malformed("proof"))?;
        let proof = decode(proof, "proof")?;
        let mut parts = without_proof.split(',');
        let channel =
            attribute(parts.next(), 'c').ok_or(ScramError::Malformed("channel binding"))?;
        if channel != binding {
            return Err(ScramError::ChannelBinding());
        }
        if attribute(parts.next(), 'r') != Some(nonce.as_str()) {
            return Err(ScramError::Nonce());
        }
        let auth_message = format!("{client_first_bare},{server_first},{without_proof}");
        let signature = hmac(verifier.stored_key(), auth_message.as_bytes());
        let client_key: Vec<u8> = proof.iter().zip(signature).map(|(p, s)| p ^ s).collect();
        let proved = proof.len() == 32 && same(&sha256(&client_key), verifier.stored_key());
        let (true, Some(principal)) = (proved, *principal) else {
            return Err(ScramError::Proof());
        };
        let server_signature = hmac(verifier.server_key(), auth_message.as_bytes());
        self.state = ServerState::Done;
        Ok((
            format!("v={}", encode(&server_signature)).into_bytes(),
            principal,
        ))
    }
}
