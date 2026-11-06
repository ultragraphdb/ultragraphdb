/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use ultragraphdb_common_service::RoleStart;
use ultragraphdb_error_role::RoleError;

use crate::types::role_secret::RoleSecret;

/// How long storage and compute retry joining when `[cluster] join_deadline_ms` does not say.
const JOIN_DEADLINE_MS: u64 = 60_000;

/// What every role reads of its cluster before it connects to anything.
#[derive(Debug, Clone)]
pub struct RoleCluster {
    id: u64,
    secret: RoleSecret,
    deadline: Duration,
}

impl RoleCluster {
    /// The cluster `id`, whose roles hold `secret` and retry joining for `deadline`.
    #[must_use]
    pub fn new(id: u64, secret: RoleSecret, deadline: Duration) -> Self {
        Self {
            id,
            secret,
            deadline,
        }
    }

    /// The cluster `start`'s `[cluster]` table names, its role secret read.
    ///
    /// # Errors
    ///
    /// [`RoleError::configuration`] naming `cluster.id` or `cluster.role_secret_file` when either is absent, and as
    /// [`RoleSecret::read`] refuses the file.
    pub fn read(start: &RoleStart) -> Result<Self, RoleError> {
        let cluster = start.config().cluster();
        let missing =
            |key: &str| RoleError::configuration(format!("the setting cluster.{key} is required"));
        let id = cluster.id().ok_or_else(|| missing("id"))?;
        let file = cluster
            .role_secret_file()
            .ok_or_else(|| missing("role_secret_file"))?;
        Ok(Self {
            id,
            secret: RoleSecret::read(file)?,
            deadline: Duration::from_millis(cluster.join_deadline_ms().unwrap_or(JOIN_DEADLINE_MS)),
        })
    }

    /// The cluster's identity, which every frame's header names.
    #[must_use]
    pub fn id(&self) -> u64 {
        self.id
    }

    /// The role secret.
    #[must_use]
    pub fn secret(&self) -> &RoleSecret {
        &self.secret
    }

    /// How long this role retries joining, and connecting back.
    #[must_use]
    pub fn deadline(&self) -> Duration {
        self.deadline
    }
}
