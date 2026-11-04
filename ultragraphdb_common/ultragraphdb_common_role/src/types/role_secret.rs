/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_scram::{SALT_SIZE, ScramVerifier};

use crate::types::role_credentials::RoleCredentials;

/// The iterations a role's verifier is derived over (`PROTOCOL.md` §11.12): the secret is a long random string, so the
/// count guards nothing a client's password needs it for, and every role connection pays it.
pub const ROLE_ITERATIONS: u32 = 4_096;

/// The cluster's role secret, as a role reads it at start. Its `Debug` holds the file it came from, never the secret.
#[derive(Clone)]
pub struct RoleSecret {
    file: String,
    password: String,
}

impl RoleSecret {
    /// The secret `file` holds: its contents without a final line break.
    ///
    /// # Errors
    ///
    /// [`RoleError::configuration`] naming `file` when it is missing, unreadable, not UTF-8 or empty.
    pub fn read(file: &str) -> Result<Self, RoleError> {
        let refused =
            |why: String| RoleError::configuration(format!("the role secret file {file}: {why}"));
        let contents = std::fs::read_to_string(file).map_err(|error| refused(error.to_string()))?;
        let line = contents.strip_suffix('\n').unwrap_or(&contents);
        let password = line.strip_suffix('\r').unwrap_or(line);
        if password.is_empty() {
            return Err(refused("it is empty".to_string()));
        }
        Ok(Self {
            file: file.to_string(),
            password: password.to_string(),
        })
    }

    /// The password a role connection authenticates with.
    #[must_use]
    pub fn password(&self) -> &str {
        &self.password
    }

    /// What this role verifies a peer against: a verifier of the secret, with a salt of 16 random bytes.
    ///
    /// # Errors
    ///
    /// [`RoleError::configuration`] naming the file when the secret cannot be prepared as a password, and
    /// [`RoleError::unavailable`] when the operating system gives no random bytes.
    pub fn credentials(&self) -> Result<RoleCredentials, RoleError> {
        let mut salt = [0u8; SALT_SIZE];
        getrandom::fill(&mut salt)
            .map_err(|error| RoleError::unavailable(format!("the random source: {error}")))?;
        let verifier =
            ScramVerifier::derive(&self.password, &salt, ROLE_ITERATIONS).map_err(|error| {
                RoleError::configuration(format!("the role secret file {}: {error}", self.file))
            })?;
        Ok(RoleCredentials::new(verifier))
    }
}

impl fmt::Debug for RoleSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RoleSecret")
            .field("file", &self.file)
            .finish_non_exhaustive()
    }
}
