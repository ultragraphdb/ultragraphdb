/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_scram::{Credentials, ScramVerifier};

/// The principal a role connection authenticates as: a role acts for no principal (`PROTOCOL.md` §11.12).
const NO_PRINCIPAL: u64 = 0;

/// The names role connections authenticate with: the roles' own.
const ROLE_NAMES: [&str; 3] = ["frontend", "compute", "storage"];

/// What a role verifies its peers against: the secret's verifier, for the three roles' names and no other. Held by
/// this role alone, since the principals live in the frontend, which is itself a peer.
#[derive(Debug, Clone)]
pub struct RoleCredentials {
    verifier: ScramVerifier,
}

impl RoleCredentials {
    pub(crate) fn new(verifier: ScramVerifier) -> Self {
        Self { verifier }
    }
}

impl Credentials for RoleCredentials {
    fn lookup(&self, name: &str) -> Option<(u64, ScramVerifier)> {
        ROLE_NAMES
            .contains(&name)
            .then(|| (NO_PRINCIPAL, self.verifier.clone()))
    }
}
