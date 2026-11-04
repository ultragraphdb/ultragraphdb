/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

/// What a role runs as: its target, its instance and the revision of its settings, which its `/health` reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleIdentity {
    target: String,
    instance: String,
    revision: u64,
}

impl RoleIdentity {
    /// Instance `instance` of `target`, at `revision`.
    pub fn new(target: impl Into<String>, instance: impl Into<String>, revision: u64) -> Self {
        Self {
            target: target.into(),
            instance: instance.into(),
            revision,
        }
    }

    /// The deployment target.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    /// The instance.
    #[must_use]
    pub fn instance(&self) -> &str {
        &self.instance
    }

    /// The revision of its settings.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }
}

impl fmt::Display for RoleIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "target {}, instance {}, revision {}",
            self.target, self.instance, self.revision
        )
    }
}
