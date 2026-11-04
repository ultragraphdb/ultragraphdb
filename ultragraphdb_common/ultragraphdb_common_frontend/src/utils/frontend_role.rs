/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use compio::runtime::spawn;
use ultragraphdb_common_role::{
    ROLE_ITERATIONS, RoleClient, RoleCluster, Server, listen, serve_connection,
};
use ultragraphdb_common_service::{Readiness, RoleKind as ServiceRole, RoleStart};
use ultragraphdb_common_service_config::EndpointSecurity;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_scram::Credentials;
use ultragraphdb_protocol_transport::{Address, Endpoint};
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, ErrorCode, ErrorReply, FLAG_LAST, FrameView, Header, Join,
    JoinView, Members, MembersView, RoleKind,
};

use crate::types::membership::Membership;
use crate::types::no_principals::NoPrincipals;

/// How long a failed connection back waits before the next.
const RETRY: std::time::Duration = std::time::Duration::from_millis(100);

/// What the frontend's tasks share, all on this one runtime: who joined, the connections back, and its readiness.
#[derive(Debug, Clone)]
struct Shared {
    cluster: RoleCluster,
    membership: Rc<RefCell<Membership>>,
    peers: Rc<RefCell<HashMap<String, RoleClient>>>,
    readiness: Readiness,
}

#[expect(
    clippy::disallowed_methods,
    reason = "a deadline in the shell, which no simulation replays"
)]
fn now() -> Instant {
    Instant::now()
}

/// The frontend role: it listens at `[frontend] role_address` for the roles joining and at `[endpoint] address`, when
/// set, for clients. It is ready once it has connected back to a partition.
///
/// # Errors
///
/// [`RoleError::configuration`] for a setting absent or out of its bounds, a refused role secret, or a client endpoint
/// asking for TLS, which this build's endpoint does not terminate; [`RoleError::unavailable`] when it cannot listen.
pub async fn frontend_role(start: RoleStart, readiness: Readiness) -> Result<(), RoleError> {
    let cluster = RoleCluster::read(&start)?;
    let config = start.config();
    let role_address = config
        .frontend()
        .role_address()
        .ok_or_else(|| RoleError::configuration("the setting frontend.role_address is required"))?;
    let partition_count = config
        .cluster()
        .partition_count()
        .filter(|count| *count > 0)
        .ok_or_else(|| {
            RoleError::configuration("the setting cluster.partition_count is at least 1")
        })?;
    let credentials = cluster.secret().credentials()?;
    let roles = Server::new(cluster.id(), "frontend", Endpoint::Frontend, credentials)?
        .with_iterations(ROLE_ITERATIONS);
    let shared = Shared {
        cluster,
        membership: Rc::new(RefCell::new(Membership::new(partition_count))),
        peers: Rc::default(),
        readiness,
    };
    let listener = listen(role_address).await?;
    spawn(accept_roles(listener, roles, shared.clone())).detach();
    if let Some(address) = config.endpoint().address() {
        match config
            .endpoint()
            .security()
            .map_err(|error| RoleError::configuration(error.to_string()))?
        {
            EndpointSecurity::Plaintext => {}
            EndpointSecurity::Tls { .. } => {
                return Err(RoleError::configuration(
                    "the client endpoint serves clear text in this build; endpoint.plaintext must be true",
                ));
            }
        }
        let clients = Server::new(
            shared.cluster.id(),
            "frontend",
            Endpoint::Client,
            NoPrincipals,
        )?;
        spawn(accept_clients(listen(address).await?, clients)).detach();
    }
    Ok(())
}

async fn accept_roles<C: Credentials + Clone + 'static>(
    listener: Listener,
    server: Server<C>,
    shared: Shared,
) {
    loop {
        match listener.accept().await {
            Ok(carrier) => {
                let (server, shared) = (server.clone(), shared.clone());
                spawn(async move {
                    serve_connection(carrier, &server, |stream, frame| {
                        answer(&shared, stream, frame)
                    })
                    .await;
                })
                .detach();
            }
            Err(error) => tracing::warn!(%error, "a role connection was not accepted"),
        }
    }
}

async fn accept_clients(listener: Listener, server: Server<NoPrincipals>) {
    loop {
        match listener.accept().await {
            Ok(carrier) => {
                let server = server.clone();
                spawn(async move {
                    serve_connection(carrier, &server, |_stream, _frame| {
                        Err(unavailable("this frontend holds no principal"))
                    })
                    .await;
                })
                .detach();
            }
            Err(error) => tracing::warn!(%error, "a client connection was not accepted"),
        }
    }
}

/// The reply to a request of the frontend's role address, or its refusal. A command the endpoint serves and this role
/// does not host yet is refused as unavailable.
fn answer(shared: &Shared, stream: u64, frame: &FrameView<'_>) -> Result<Vec<u8>, ErrorReply> {
    let command = frame.command();
    let header = Header::new(shared.cluster.id(), command)
        .with_stream(stream)
        .with_flags(FLAG_LAST);
    if command == Command::Join {
        let join = JoinView::from_frame(frame).map_err(|rule| ErrorReply::protocol(&rule))?;
        shared.membership.borrow_mut().join(&join)?;
        let reply = joined(&join);
        spawn(connect_back(
            shared.clone(),
            join.address().to_string(),
            join.kind(),
        ))
        .detach();
        return built(&reply, header);
    }
    if command == Command::Members {
        let asked = MembersView::from_frame(frame).map_err(|rule| ErrorReply::protocol(&rule))?;
        if !asked.is_empty() {
            return Err(ErrorReply::new(
                ErrorCode::Protocol,
                format!("a Members request holds {} members", asked.len()),
            ));
        }
        let members: Members = shared.membership.borrow().members();
        return built(&members, header);
    }
    Err(unavailable(&format!(
        "command {} is not hosted here yet",
        command.number()
    )))
}

/// The Join's answer: the same fields.
fn joined(join: &JoinView<'_>) -> Join {
    if join.kind() == RoleKind::Compute {
        return Join::compute(join.address());
    }
    let partitions = (0..join.partition_count())
        .filter_map(|index| join.partition(index))
        .collect::<Vec<u32>>();
    Join::storage(join.address(), partitions)
}

fn built<B: BodyBuilder>(body: &B, header: Header) -> Result<Vec<u8>, ErrorReply> {
    body.frame(header)
        .map_err(|rule| ErrorReply::protocol(&rule))
}

fn unavailable(message: &str) -> ErrorReply {
    ErrorReply::new(ErrorCode::Unavailable, message)
}

/// Connects back to the role that joined at `text`, retrying until the deadline; the first partition reached makes the
/// frontend ready. A connection a later Join of the address brings replaces this one.
async fn connect_back(shared: Shared, text: String, kind: RoleKind) {
    let Ok(address) = Address::parse(&text) else {
        tracing::warn!(address = %text, "a role joined at an address that does not parse");
        return;
    };
    let due = now() + shared.cluster.deadline();
    let name = ServiceRole::Frontend.to_string();
    loop {
        let secret = shared.cluster.secret().password();
        match RoleClient::dial(&address, shared.cluster.id(), &name, secret).await {
            Ok(client) => {
                let _replaced = shared.peers.borrow_mut().insert(text, client);
                if kind == RoleKind::Storage {
                    shared.readiness.set(true);
                }
                return;
            }
            Err(error) if now() >= due => {
                tracing::warn!(address = %text, %error, "the frontend could not connect back");
                return;
            }
            Err(_) => compio::time::sleep(RETRY).await,
        }
    }
}
