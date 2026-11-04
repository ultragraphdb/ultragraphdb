/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::net::SocketAddr;

use ultragraphdb_common_service_config::{ServiceConfig, ServiceConfigError};
use ultragraphdb_error_role::RoleError;

use crate::types::bootstrap::Bootstrap;
use crate::types::role_identity::RoleIdentity;

/// What a role has read before it starts anything: its settings and where its `/health` and `/ready` listen.
#[derive(Debug, Clone)]
pub struct RoleStart {
    config: ServiceConfig,
    observability: SocketAddr,
    identity: RoleIdentity,
    bootstrap: Option<Bootstrap>,
}

impl RoleStart {
    /// The start of a role running as `identity`, on the settings `bytes` hold.
    ///
    /// # Errors
    ///
    /// [`RoleError::configuration`] for settings this release refuses, naming the setting, or an absent or malformed
    /// `observability.address`.
    pub fn from_settings(bytes: &[u8], identity: RoleIdentity) -> Result<Self, RoleError> {
        let config = ServiceConfig::parse(bytes)
            .map_err(|error| RoleError::configuration(format!("{identity}: {error}")))?;
        let address = config.observability().address().ok_or_else(|| {
            RoleError::configuration(format!(
                "{identity}: {}",
                ServiceConfigError::missing("observability", "address")
            ))
        })?;
        let observability = address
            .strip_prefix("tcp://")
            .and_then(|host_port| host_port.parse().ok())
            .ok_or_else(|| {
                RoleError::configuration(format!(
                    "{identity}: the setting observability.address holds {address}; tcp://<ip>:<port> is expected"
                ))
            })?;
        Ok(Self {
            config,
            observability,
            identity,
            bootstrap: None,
        })
    }

    /// The start, able to fetch again through `bootstrap`, which a load does.
    #[must_use]
    pub fn with_bootstrap(mut self, bootstrap: Bootstrap) -> Self {
        self.bootstrap = Some(bootstrap);
        self
    }

    /// What a load fetches through.
    pub fn bootstrap(&self) -> Option<&Bootstrap> {
        self.bootstrap.as_ref()
    }

    /// What it runs as.
    pub fn identity(&self) -> &RoleIdentity {
        &self.identity
    }

    /// The role's settings.
    pub fn config(&self) -> &ServiceConfig {
        &self.config
    }

    /// The address `/health` and `/ready` listen at.
    pub fn observability(&self) -> SocketAddr {
        self.observability
    }
}
