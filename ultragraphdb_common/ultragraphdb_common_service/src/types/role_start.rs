/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::net::SocketAddr;
use std::path::PathBuf;

use ultragraphdb_common_service_config::{ServiceConfig, ServiceConfigError};
use ultragraphdb_error_role::RoleError;

/// What a role has read before it starts anything: its settings and where its `/health` and `/ready` listen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleStart {
    config: ServiceConfig,
    observability: SocketAddr,
}

impl RoleStart {
    /// The start named by `args`, the program's name left out, `--config <file>` or nothing, and the environment
    /// `variables`.
    ///
    /// # Errors
    ///
    /// [`RoleError::configuration`] for any other argument, a refused configuration, or an absent or malformed
    /// `observability.address`.
    pub fn read(
        args: &[String],
        variables: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Self, RoleError> {
        let file = match args {
            [] => None,
            [flag, file] if flag == "--config" => Some(PathBuf::from(file)),
            other => {
                return Err(RoleError::configuration(format!(
                    "the arguments are --config <file> or nothing, not {other:?}"
                )));
            }
        };
        let config =
            ServiceConfig::load(file.as_deref(), variables).map_err(|error| refused(&error))?;
        let address = config
            .observability()
            .address()
            .ok_or_else(|| refused(&ServiceConfigError::missing("observability", "address")))?;
        let observability = address
            .strip_prefix("tcp://")
            .and_then(|host_port| host_port.parse().ok())
            .ok_or_else(|| {
                RoleError::configuration(format!(
                    "the setting observability.address holds {address}; tcp://<ip>:<port> is expected"
                ))
            })?;
        Ok(Self {
            config,
            observability,
        })
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

fn refused(error: &ServiceConfigError) -> RoleError {
    RoleError::configuration(error.to_string())
}
