/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_scram::ScramVerifier;

/// A principal of the cluster: its identity, numbered from 1 and never reused, its name, its SCRAM-SHA-256 verifier
/// (salt, iteration count, stored key and server key), never the password, and whether it holds the administrator
/// role, `cluster_admin`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Principal {
    id: u64,
    name: String,
    verifier: ScramVerifier,
    administrator: bool,
}

impl Principal {
    /// The principal `id` named `name`, authenticating with `verifier`, holding the administrator role when
    /// `administrator`.
    #[must_use]
    pub fn new(id: u64, name: String, verifier: ScramVerifier, administrator: bool) -> Self {
        Self {
            id,
            name,
            verifier,
            administrator,
        }
    }

    /// The principal's identity.
    #[must_use]
    pub fn id(&self) -> u64 {
        self.id
    }

    /// The principal's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The principal's SCRAM-SHA-256 verifier.
    #[must_use]
    pub fn verifier(&self) -> &ScramVerifier {
        &self.verifier
    }

    /// Whether the principal holds the administrator role.
    #[must_use]
    pub fn administrator(&self) -> bool {
        self.administrator
    }
}
