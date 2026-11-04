/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;

use compio::runtime::spawn;
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::SequencerConfig;
use ultragraphdb_common_role::{
    ROLE_ITERATIONS, RoleCluster, RoleCredentials, Server, listen, serve_connection,
};
use ultragraphdb_common_service::{Readiness, RoleStart};
use ultragraphdb_common_service_config::{EndpointSecurity, ServiceConfig};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::Endpoint;
use ultragraphdb_protocol_transport_compio::Listener;

use crate::types::asked_credentials::AskedCredentials;
use crate::types::client_session::ClientSession;
use crate::types::owner_handle::OwnerHandle;
use crate::types::owner_start::OwnerStart;
use crate::types::role_session::{RoleSession, RoleShared};
use crate::utils::owner_thread::start_owner;

/// The frontend role: it recovers its owner, then listens at `[frontend] role_address` for the roles joining and at
/// `[endpoint] address`, when set, for clients. It is ready once it has connected back to a partition, and runs until
/// its owner stops.
///
/// # Errors
///
/// [`RoleError::configuration`] for a setting absent or out of its bounds, a refused role secret, or a client endpoint
/// asking for TLS, which this build's endpoint does not terminate; [`RoleError::unavailable`] when it cannot listen;
/// what recovery refused or the owner stopped with.
pub async fn frontend_role(start: RoleStart, readiness: Readiness) -> Result<(), RoleError> {
    let cluster = RoleCluster::read(&start)?;
    let config = start.config();
    let role_address = required(config.frontend().role_address(), "frontend.role_address")?;
    let data_dir = PathBuf::from(required(config.frontend().data_dir(), "frontend.data_dir")?);
    let credentials = cluster.secret().credentials()?;
    let owner_start = owner_start(config)?;
    let owner = start_owner(
        config.clone(),
        cluster.id(),
        data_dir,
        owner_start,
        Vec::new(),
    )
    .await?;
    let (handle, stopped) = owner.into_parts();
    let roles =
        Server::new(cluster.id(), "frontend", Endpoint::Frontend)?.with_iterations(ROLE_ITERATIONS);
    let shared = RoleShared::new(cluster.clone(), handle.clone(), readiness);
    spawn(accept_roles(
        listen(role_address).await?,
        roles,
        credentials,
        shared,
    ))
    .detach();
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
        let clients = Server::new(cluster.id(), "frontend", Endpoint::Client)?;
        spawn(accept_clients(listen(address).await?, clients, handle)).detach();
    }
    stopped.await.unwrap_or_else(|_| {
        Err(RoleError::software(
            "the owner's thread ended without a word",
        ))
    })
}

fn required<'a>(value: Option<&'a str>, setting: &str) -> Result<&'a str, RoleError> {
    value.ok_or_else(|| RoleError::configuration(format!("the setting {setting} is required")))
}

/// What the settings say the owner is, each bound the state machines' own.
fn owner_start(config: &ServiceConfig) -> Result<OwnerStart, RoleError> {
    let refused = |error: &dyn std::fmt::Display| RoleError::configuration(error.to_string());
    let partition_count = config
        .cluster()
        .partition_count()
        .filter(|count| *count > 0)
        .ok_or_else(|| {
            RoleError::configuration("the setting cluster.partition_count is at least 1")
        })?;
    let sequencer_table = config.sequencer();
    let required_ms = |value: Option<u64>, setting: &str| {
        value.ok_or_else(|| RoleError::configuration(format!("the setting {setting} is required")))
    };
    let sequencer = SequencerConfig::new(
        required_ms(sequencer_table.epoch_ms_min(), "sequencer.epoch_ms_min")?,
        required_ms(sequencer_table.epoch_ms_max(), "sequencer.epoch_ms_max")?,
    )
    .map_err(|error| refused(&error))?;
    let table = config.metadata();
    let mut metadata = MetadataConfig::new();
    if let Some(size) = table.block_size_max() {
        metadata = metadata
            .with_block_size_max(size)
            .map_err(|error| refused(&error))?;
    }
    if let Some(sessions) = table.sessions_max() {
        metadata = metadata
            .with_sessions_max(sessions)
            .map_err(|error| refused(&error))?;
    }
    if let Some(rounds) = table.snapshot_rounds() {
        metadata = metadata
            .with_snapshot_rounds(rounds)
            .map_err(|error| refused(&error))?;
    }
    Ok(OwnerStart::new(metadata, sequencer, partition_count))
}

async fn accept_roles(
    listener: Listener,
    server: Server,
    credentials: RoleCredentials,
    shared: RoleShared,
) {
    loop {
        match listener.accept().await {
            Ok(carrier) => {
                let (server, credentials) = (server.clone(), credentials.clone());
                let session = RoleSession::new(shared.clone());
                spawn(async move {
                    serve_connection(carrier, &server, credentials, session).await;
                })
                .detach();
            }
            Err(error) => tracing::warn!(%error, "a role connection was not accepted"),
        }
    }
}

async fn accept_clients(listener: Listener, server: Server, owner: OwnerHandle) {
    loop {
        match listener.accept().await {
            Ok(carrier) => {
                let server = server.clone();
                let credentials = AskedCredentials::default();
                let session = ClientSession::new(owner.clone(), credentials.clone());
                spawn(async move {
                    serve_connection(carrier, &server, credentials, session).await;
                })
                .detach();
            }
            Err(error) => tracing::warn!(%error, "a client connection was not accepted"),
        }
    }
}
