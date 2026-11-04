/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::runtime::spawn;
use ultragraphdb_common_role::{
    Answer, ROLE_ITERATIONS, RoleClient, RoleCluster, RoleCredentials, Server, Session,
    frontend_address, listen, retry_until, serve_connection,
};
use ultragraphdb_common_service::{Readiness, RoleKind, RoleStart, Stopping};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::{Address, Endpoint, ReceivedFrame, TransportError};
use ultragraphdb_protocol_wire::{BodyView, ErrorCode, ErrorReply, Join, Members, MembersView};

/// The compute role's connections: it listens at `[compute] role_address`, joins the frontend, and is ready once the
/// members it reads name a partition to read from.
///
/// # Errors
///
/// [`RoleError::configuration`] for a setting absent or a refused role secret; [`RoleError::unavailable`] when it
/// cannot listen, or has not joined by its deadline.
pub(crate) async fn compute_role(
    start: RoleStart,
    readiness: Readiness,
    stopping: Stopping,
) -> Result<(), RoleError> {
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
        &stopping,
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
    stopping: &Stopping,
) -> Result<(), RoleError> {
    let cluster = RoleCluster::read(start)?;
    let frontend = frontend_address(start)?;
    let credentials: RoleCredentials = cluster.secret().credentials()?;
    let server = Server::new(cluster.id(), "role", endpoint)?.with_iterations(ROLE_ITERATIONS);
    let listener = listen(own).await?;
    spawn(async move {
        loop {
            match listener.accept().await {
                Ok(carrier) => {
                    let (server, credentials) = (server.clone(), credentials.clone());
                    spawn(async move {
                        serve_connection(carrier, &server, credentials, NotHosted).await;
                    })
                    .detach();
                }
                Err(error) => tracing::warn!(%error, "a role connection was not accepted"),
            }
        }
    })
    .detach();
    let name = kind.to_string();
    let joined = retry_until(&cluster, &kind.to_string(), stopping, || {
        attempt(&frontend, &cluster, &name, join)
    })
    .await?;
    if joined.is_some() {
        readiness.set(true);
    }
    Ok(())
}

/// One attempt: dial, authenticate, Join, read the Members until a partition has joined.
async fn attempt(
    frontend: &Address,
    cluster: &RoleCluster,
    name: &str,
    join: &Join,
) -> Result<(), TransportError> {
    let mut client =
        RoleClient::dial(frontend, cluster.id(), name, cluster.secret().password()).await?;
    drop(client.request(join).await?);
    let answer = client.request(&Members::new()).await?;
    let view = answer.view().map_err(TransportError::protocol)?;
    let members = MembersView::from_frame(&view).map_err(TransportError::protocol)?;
    let partitions = (0..members.len())
        .filter_map(|index| members.member(index))
        .filter(|member| member.partition() > 0)
        .count();
    if partitions == 0 {
        return Err(TransportError::authentication(
            "no partition has joined yet",
        ));
    }
    Ok(())
}

/// A role connection to a role that hosts nothing yet: it authenticates, and refuses every request.
#[derive(Debug, Clone, Copy)]
struct NotHosted;

impl Session for NotHosted {
    async fn answer(&mut self, _stream: u64, frame: ReceivedFrame) -> Answer {
        Answer::Refuse(ErrorReply::new(
            ErrorCode::Unavailable,
            format!("command {} is not hosted here yet", frame.command()),
        ))
    }
}
