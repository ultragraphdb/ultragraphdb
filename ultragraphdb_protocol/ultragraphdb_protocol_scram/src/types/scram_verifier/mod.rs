/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::scram_error::ScramError;
use crate::utils::constants::{MAX_ITERATIONS, MIN_ITERATIONS, SALT_SIZE};
use crate::utils::crypto::{hmac, prepare, salted, same, sha256};

/// What a server stores of a password (RFC 5802 §3): the salt, the iteration count, `StoredKey`, the SHA-256 of the
/// client key, and `ServerKey`. A password cannot be read back from it, and a stolen verifier does not log in.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScramVerifier {
    salt: Vec<u8>,
    iterations: u32,
    stored_key: [u8; 32],
    server_key: [u8; 32],
}

impl ScramVerifier {
    /// The verifier of a stored principal, as its store holds it.
    #[must_use]
    pub fn new(salt: Vec<u8>, iterations: u32, stored_key: [u8; 32], server_key: [u8; 32]) -> Self {
        Self {
            salt,
            iterations,
            stored_key,
            server_key,
        }
    }

    /// The verifier of `password`, prepared with SASLprep, salted with `salt` over `iterations` of PBKDF2-HMAC-SHA256.
    ///
    /// # Errors
    ///
    /// [`ScramError::prepare`] for a password SASLprep refuses and [`ScramError::iterations`] outside
    /// [`crate::MIN_ITERATIONS`] to [`crate::MAX_ITERATIONS`], a count no client would run.
    pub fn derive(password: &str, salt: &[u8], iterations: u32) -> Result<Self, ScramError> {
        if !(MIN_ITERATIONS..=MAX_ITERATIONS).contains(&iterations) {
            return Err(ScramError::iterations(iterations));
        }
        let salted = salted(&prepare(password)?, salt, iterations);
        Ok(Self::new(
            salt.to_vec(),
            iterations,
            sha256(&hmac(&salted, b"Client Key")),
            hmac(&salted, b"Server Key"),
        ))
    }

    /// The verifier of `name`, which no principal holds, derived from the server's `secret` at `iterations`: the same
    /// for the same secret and name, so a name's salt tells no prober whether it exists, and its keys verify no proof
    /// a client computes without the secret (`PROTOCOL.md` §13.8).
    #[must_use]
    pub fn unknown(secret: &[u8; 32], name: &str, iterations: u32) -> Self {
        let mut salt = hmac(secret, format!("salt:{name}").as_bytes()).to_vec();
        salt.truncate(SALT_SIZE);
        Self::new(
            salt,
            iterations,
            hmac(secret, format!("stored:{name}").as_bytes()),
            hmac(secret, format!("server:{name}").as_bytes()),
        )
    }

    /// The group's half of the exchange (RFC 5802 §3): the ServerSignature of `auth_message` when `proof` is the
    /// ClientProof this verifier's `StoredKey` checks, compared in constant time; `None` when it is not, which is also
    /// the answer for the verifier of a name no principal holds.
    #[must_use]
    pub fn sign(&self, auth_message: &str, proof: &[u8; 32]) -> Option<[u8; 32]> {
        // RFC 5802 §3: ClientKey = ClientProof XOR HMAC(StoredKey, AuthMessage), and the proof holds when
        // H(ClientKey) is StoredKey.
        let mut client_key = *proof;
        client_key
            .iter_mut()
            .zip(hmac(&self.stored_key, auth_message.as_bytes()))
            .for_each(|(key, signature)| *key ^= signature);
        same(&sha256(&client_key), &self.stored_key)
            .then(|| hmac(&self.server_key, auth_message.as_bytes()))
    }

    /// The salt the password was derived with.
    #[must_use]
    pub fn salt(&self) -> &[u8] {
        &self.salt
    }

    /// The iteration count the password was derived over.
    #[must_use]
    pub fn iterations(&self) -> u32 {
        self.iterations
    }

    /// `StoredKey`, the SHA-256 of the client key.
    #[must_use]
    pub fn stored_key(&self) -> &[u8; 32] {
        &self.stored_key
    }

    /// `ServerKey`, which signs the server's final message.
    #[must_use]
    pub fn server_key(&self) -> &[u8; 32] {
        &self.server_key
    }
}
