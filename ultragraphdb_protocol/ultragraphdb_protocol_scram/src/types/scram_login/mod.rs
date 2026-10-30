/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod scram_login_login;

use crate::errors::scram_error::ScramError;
use crate::types::scram_client::ScramClient;

/// The nonce a login's name and password are prepared with to check them, before any exchange draws a real one.
const PROBE: &str = "probe";

/// A client's login by SCRAM-SHA-256: a principal's name and password, and `nonces`, the source of each exchange's
/// client nonce, [`crate::system_nonce`] or the caller's own. Each exchange, the first and every re-authentication,
/// is a fresh [`crate::ScramClient`] with the next nonce.
#[derive(Debug, Clone)]
pub struct ScramLogin<N> {
    name: String,
    password: String,
    nonces: N,
}

impl<N: FnMut() -> Result<String, ScramError>> ScramLogin<N> {
    /// The login of `name` with `password`, refused as [`ScramError::Prepare`] when SASLprep refuses either, so a
    /// login that is built can start every exchange its nonces allow.
    pub fn new(name: &str, password: &str, nonces: N) -> Result<Self, ScramError> {
        ScramClient::new(name, password, PROBE)?;
        Ok(Self {
            name: name.to_string(),
            password: password.to_string(),
            nonces,
        })
    }

    /// A client of the login's name and password with `nonce`.
    pub(crate) fn client(&self, nonce: &str) -> Result<ScramClient, ScramError> {
        ScramClient::new(&self.name, &self.password, nonce)
    }

    /// The login's next nonce.
    pub(crate) fn next_nonce(&mut self) -> Result<String, ScramError> {
        (self.nonces)()
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}
