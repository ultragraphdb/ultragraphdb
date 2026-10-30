/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod scram_client_mechanism;

use crate::errors::scram_error::ScramError;
use crate::utils::constants::MIN_ITERATIONS;
use crate::utils::crypto::{hmac, prepare, salted, same, sha256};
use crate::utils::message::{attribute, decode, encode, escape, server_error, valid_nonce};

/// The GS2 header of a client without channel binding.
const GS2_HEADER: &str = "n,,";

/// Where a client's exchange stands.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ClientState {
    Initial,
    First { client_first_bare: String },
    Final { server_signature: [u8; 32] },
    Done,
}

/// The client's side of SCRAM-SHA-256 (RFC 5802 §5): the client-first-message, the client-final-message answering
/// the server's challenge with the proof, and the check of the server's signature. The client's nonce is the caller's,
/// printable ASCII without a comma, drawn from a source the caller trusts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScramClient {
    name: String,
    password: String,
    nonce: String,
    state: ClientState,
}

impl ScramClient {
    /// The exchange of `name` with `password`, both prepared with SASLprep, and the client's `nonce`; refused as
    /// [`ScramError::Prepare`] for a name or password SASLprep refuses and [`ScramError::Malformed`] for a nonce
    /// that is empty or not printable ASCII without a comma.
    pub fn new(name: &str, password: &str, nonce: &str) -> Result<Self, ScramError> {
        let (name, password) = (prepare(name)?, prepare(password)?);
        if !valid_nonce(nonce) {
            return Err(ScramError::Malformed("nonce"));
        }
        Ok(Self {
            name,
            password,
            nonce: nonce.to_string(),
            state: ClientState::Initial,
        })
    }

    /// The client-first-message: `n,,n=<name>,r=<nonce>`, the name's `=` and `,` escaped.
    pub fn first_message(&mut self) -> Result<Vec<u8>, ScramError> {
        if self.state != ClientState::Initial {
            return Err(ScramError::OutOfStep());
        }
        let client_first_bare = format!("n={},r={}", escape(&self.name), self.nonce);
        let message = format!("{GS2_HEADER}{client_first_bare}").into_bytes();
        self.state = ClientState::First { client_first_bare };
        Ok(message)
    }

    /// The client-final-message answering `server_first`: the channel binding `biws`, the server's nonce, and the
    /// proof. Refused as [`ScramError::Malformed`], [`ScramError::Nonce`] when the server's nonce does not extend the
    /// client's, [`ScramError::Iterations`] below [`crate::MIN_ITERATIONS`], or [`ScramError::OutOfStep`].
    pub fn final_message(&mut self, server_first: &[u8]) -> Result<Vec<u8>, ScramError> {
        let ClientState::First { client_first_bare } = &self.state else {
            return Err(ScramError::OutOfStep());
        };
        let text = std::str::from_utf8(server_first).map_err(|_| ScramError::Malformed("nonce"))?;
        if let Some(error) = server_error(text) {
            return Err(error);
        }
        let mut parts = text.split(',');
        let nonce = attribute(parts.next(), 'r').ok_or(ScramError::Malformed("nonce"))?;
        if nonce.len() <= self.nonce.len() || !nonce.starts_with(&self.nonce) || !valid_nonce(nonce)
        {
            return Err(ScramError::Nonce());
        }
        let salt = decode(
            attribute(parts.next(), 's').ok_or(ScramError::Malformed("salt"))?,
            "salt",
        )?;
        let iterations: u32 = attribute(parts.next(), 'i')
            .and_then(|i| i.parse().ok())
            .ok_or(ScramError::Malformed("iteration count"))?;
        if iterations < MIN_ITERATIONS {
            return Err(ScramError::Iterations(iterations));
        }
        let salted = salted(&self.password, &salt, iterations);
        let client_key = hmac(&salted, b"Client Key");
        let without_proof = format!("c={},r={nonce}", encode(GS2_HEADER.as_bytes()));
        let auth_message = format!("{client_first_bare},{text},{without_proof}");
        let signature = hmac(&sha256(&client_key), auth_message.as_bytes());
        let proof: Vec<u8> = client_key
            .iter()
            .zip(signature)
            .map(|(k, s)| k ^ s)
            .collect();
        let server_signature = hmac(&hmac(&salted, b"Server Key"), auth_message.as_bytes());
        self.state = ClientState::Final { server_signature };
        Ok(format!("{without_proof},p={}", encode(&proof)).into_bytes())
    }

    /// The server-final-message checked: its signature the one the password's server key gives, compared in constant
    /// time; refused as [`ScramError::ServerSignature`], [`ScramError::Server`] for its `e=`, or
    /// [`ScramError::OutOfStep`].
    pub fn verify(&mut self, server_final: &[u8]) -> Result<(), ScramError> {
        let ClientState::Final { server_signature } = &self.state else {
            return Err(ScramError::OutOfStep());
        };
        let text =
            std::str::from_utf8(server_final).map_err(|_| ScramError::Malformed("verifier"))?;
        if let Some(error) = server_error(text) {
            return Err(error);
        }
        let signature = decode(
            attribute(Some(text), 'v').ok_or(ScramError::Malformed("verifier"))?,
            "verifier",
        )?;
        if !same(&signature, server_signature) {
            return Err(ScramError::ServerSignature());
        }
        self.state = ClientState::Done;
        Ok(())
    }
}
