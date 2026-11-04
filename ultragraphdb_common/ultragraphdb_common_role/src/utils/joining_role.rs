/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Instant;

use compio::runtime::spawn;
use ultragraphdb_common_service::{Readiness, RoleKind, RoleStart};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::{Address, Endpoint, TransportError};
use ultragraphdb_protocol_wire::{BodyView, ErrorCode, ErrorReply, Join, Members, MembersView};

use crate::types::role_client::RoleClient;
use crate::types::role_cluster::RoleCluster;
use crate::types::role_credentials::RoleCredentials;
use crate::types::role_secret::ROLE_ITERATIONS;
use crate::utils::serve::{Server, listen, serve_connection};

/// How long a refused or failed attempt waits before the next.
const RETRY: std::time::Duration = std::time::Duration::from_millis(100);

#[expect(
    clippy::disallowed_methods,
    reason = "a deadline of the process's own start, in the shell"
)]
fn now() -> Instant {
    Instant::now()
}

/// The storage role's connections: it listens at `[storage] role_address`, joins the frontend with the partitions
/// `partition_first` and `partition_count` name, and is ready once it has joined.
///
/// # Errors
///
/// [`RoleError::configuration`] for a setting absent or out of its bounds or a refused role secret;
/// [`RoleError::unavailable`] when it cannot listen, or has not joined by its deadline.
pub async fn storage_role(start: RoleStart, readiness: Readiness) -> Result<(), RoleError> {
    let table = start.config().storage();
    let address = required(table.role_address(), "storage.role_address")?.to_string();
    let first = table.partition_first().unwrap_or(1);
    let count = table
        .partition_count()
        .filter(|count| *count > 0)
        .ok_or_else(|| {
            RoleError::configuration("the setting storage.partition_count is at least 1")
        })?;
    let partitions: Vec<u32> = (0..count).filter_map(|at| first.checked_add(at)).collect();
    let join = Join::storage(address.as_str(), partitions);
    join_as(
        RoleKind::Storage,
        Endpoint::Partition,
        &start,
        &address,
        &join,
        &readiness,
    )
    .await
}

/// The compute role's connections: it listens at `[compute] role_address`, joins the frontend, and is ready once the
/// members it reads name a partition to read from.
///
/// # Errors
///
/// [`RoleError::configuration`] for a setting absent or a refused role secret; [`RoleError::unavailable`] when it
/// cannot listen, or has not joined by its deadline.
pub async fn compute_role(start: RoleStart, readiness: Readiness) -> Result<(), RoleError> {
    let address = required(
        start.config().compute().role_address(),
        "compute.role_address",
    )?
    .to_string();
    let join = Join::compute(address.as_str());
    join_as(
        RoleKind::Compute,
        Endpoint::Compute,
        &start,
        &address,
        &join,
        &readiness,
    )
    .await
}

fn required<'a>(value: Option<&'a str>, setting: &str) -> Result<&'a str, RoleError> {
    value.ok_or_else(|| RoleError::configuration(format!("the setting {setting} is required")))
}

async fn join_as(
    kind: RoleKind,
    endpoint: Endpoint,
    start: &RoleStart,
    own: &str,
    join: &Join,
    readiness: &Readiness,
) -> Result<(), RoleError> {
    let cluster = RoleCluster::read(start)?;
    let frontend = required(
        start.config().cluster().frontend_address(),
        "cluster.frontend_address",
    )?;
    let frontend = Address::parse(frontend)
        .map_err(|error| RoleError::configuration(format!("the address {frontend}: {error}")))?;
    let credentials: RoleCredentials = cluster.secret().credentials()?;
    let server =
        Server::new(cluster.id(), "role", endpoint, credentials)?.with_iterations(ROLE_ITERATIONS);
    let listener = listen(own).await?;
    spawn(async move {
        loop {
            match listener.accept().await {
                Ok(carrier) => {
                    let server = server.clone();
                    spawn(async move {
                        serve_connection(carrier, &server, |_stream, frame| {
                            Err(ErrorReply::new(
                                ErrorCode::Unavailable,
                                format!(
                                    "command {} is not hosted here yet",
                                    frame.command().number()
                                ),
                            ))
                        })
                        .await;
                    })
                    .detach();
                }
                Err(error) => tracing::warn!(%error, "a role connection was not accepted"),
            }
        }
    })
    .detach();
    let name = kind.to_string();
    let due = now() + cluster.deadline();
    let mut attempts = 0u32;
    loop {
        attempts += 1;
        match attempt(&frontend, &cluster, &name, join).await {
            Ok(members) if kind == RoleKind::Storage || members > 0 => {
                readiness.set(true);
                tracing::info!(role = %kind, attempts, "joined");
                return Ok(());
            }
            Ok(_) => tracing::debug!(role = %kind, "no partition has joined yet"),
            Err(error) => {
                tracing::warn!(role = %kind, %error, "joining was refused or failed");
                if now() >= due {
                    return Err(RoleError::unavailable(format!(
                        "the frontend did not take this {kind} within {} ms, after {attempts} attempts: {error}",
                        cluster.deadline().as_millis()
                    )));
                }
            }
        }
        if now() >= due {
            return Err(RoleError::unavailable(format!(
                "no partition joined within {} ms, after {attempts} attempts",
                cluster.deadline().as_millis()
            )));
        }
        compio::time::sleep(RETRY).await;
    }
}

/// One attempt: dial, authenticate, Join, read the Members; the partitions the members name.
async fn attempt(
    frontend: &Address,
    cluster: &RoleCluster,
    name: &str,
    join: &Join,
) -> Result<usize, TransportError> {
    let mut client =
        RoleClient::dial(frontend, cluster.id(), name, cluster.secret().password()).await?;
    drop(client.request(join).await?);
    let answer = client.request(&Members::new()).await?;
    let view = answer.view().map_err(TransportError::protocol)?;
    let members = MembersView::from_frame(&view).map_err(TransportError::protocol)?;
    Ok((0..members.len())
        .filter_map(|index| members.member(index))
        .filter(|member| member.partition() > 0)
        .count())
}
