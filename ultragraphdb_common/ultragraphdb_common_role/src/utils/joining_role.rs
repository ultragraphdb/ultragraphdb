/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::Future;
use std::time::Instant;

use ultragraphdb_common_service::{RoleStart, Stopping};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::{Address, TransportError};

use crate::types::role_cluster::RoleCluster;

/// How long a refused or failed attempt waits before the next.
const RETRY: std::time::Duration = std::time::Duration::from_millis(100);

#[expect(
    clippy::disallowed_methods,
    reason = "a deadline of the process's own start, in the shell"
)]
fn now() -> Instant {
    Instant::now()
}

fn required<'a>(value: Option<&'a str>, setting: &str) -> Result<&'a str, RoleError> {
    value.ok_or_else(|| RoleError::configuration(format!("the setting {setting} is required")))
}

/// The frontend's role address the settings of `start` name.
///
/// # Errors
///
/// [`RoleError::configuration`] when `cluster.frontend_address` is absent or does not parse.
pub fn frontend_address(start: &RoleStart) -> Result<Address, RoleError> {
    let frontend = required(
        start.config().cluster().frontend_address(),
        "cluster.frontend_address",
    )?;
    Address::parse(frontend)
        .map_err(|error| RoleError::configuration(format!("the address {frontend}: {error}")))
}

/// The system group's role address the settings of `start` name, which every role dials.
///
/// # Errors
///
/// [`RoleError::configuration`] naming `cluster.system_address` when it is absent or does not parse.
pub fn system_address(start: &RoleStart) -> Result<Address, RoleError> {
    let system = required(
        start.config().cluster().system_address(),
        "cluster.system_address",
    )?;
    Address::parse(system).map_err(|error| {
        RoleError::configuration(format!(
            "the setting cluster.system_address holds {system}: {error}"
        ))
    })
}

/// Runs `attempt` until it succeeds, until the service stops, which gives `None`, or until the cluster's deadline.
///
/// A future dropped at an attempt drops that attempt's future, leaving what it leaves; one dropped at a pause leaves
/// nothing.
///
/// # Errors
///
/// [`RoleError::unavailable`] naming `role`, the attempts made and the last refusal, past the deadline.
pub async fn retry_until<T, F, A>(
    cluster: &RoleCluster,
    role: &str,
    stopping: &Stopping,
    mut attempt: A,
) -> Result<Option<T>, RoleError>
where
    A: FnMut() -> F,
    F: Future<Output = Result<T, TransportError>>,
{
    // A deadline past what the clock can name never passes: `join_deadline_ms` is a setting, so no sum of it may panic.
    let due = now().checked_add(cluster.deadline());
    let mut attempts = 0u32;
    while !stopping.is_stopping() {
        attempts = attempts.saturating_add(1);
        match attempt().await {
            Ok(done) => return Ok(Some(done)),
            Err(error) => {
                tracing::warn!(role, %error, "joining was refused or failed");
                if due.is_some_and(|due| now() >= due) {
                    return Err(RoleError::unavailable(format!(
                        "the frontend did not take this {role} within {} ms, after {attempts} attempts: {error}",
                        cluster.deadline().as_millis()
                    )));
                }
            }
        }
        compio::time::sleep(RETRY).await;
    }
    Ok(None)
}
