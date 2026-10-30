/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::scram_error::ScramError;
use crate::utils::constants::MIN_ITERATIONS;
use crate::utils::crypto::{hmac, prepare, salted, sha256};

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
    pub fn new(salt: Vec<u8>, iterations: u32, stored_key: [u8; 32], server_key: [u8; 32]) -> Self {
        Self {
            salt,
            iterations,
            stored_key,
            server_key,
        }
    }

    /// The verifier of `password`, prepared with SASLprep, salted with `salt` over `iterations` of PBKDF2-HMAC-SHA256;
    /// refused as [`ScramError::Prepare`] for a password SASLprep refuses and [`ScramError::Iterations`] below
    /// [`crate::MIN_ITERATIONS`].
    pub fn derive(password: &str, salt: &[u8], iterations: u32) -> Result<Self, ScramError> {
        if iterations < MIN_ITERATIONS {
            return Err(ScramError::Iterations(iterations));
        }
        let salted = salted(&prepare(password)?, salt, iterations);
        Ok(Self::new(
            salt.to_vec(),
            iterations,
            sha256(&hmac(&salted, b"Client Key")),
            hmac(&salted, b"Server Key"),
        ))
    }

    pub fn salt(&self) -> &[u8] {
        &self.salt
    }

    pub fn iterations(&self) -> u32 {
        self.iterations
    }

    pub fn stored_key(&self) -> &[u8; 32] {
        &self.stored_key
    }

    pub fn server_key(&self) -> &[u8; 32] {
        &self.server_key
    }
}
