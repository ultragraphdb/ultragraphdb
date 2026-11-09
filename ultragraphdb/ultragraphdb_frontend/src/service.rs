/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use compio::runtime::spawn;
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::SequencerConfig;
use ultragraphdb_common_frontend::{
    AskedCredentials, ClientSession, EndpointCore, EndpointLimits, EveryAuthenticated, OwnerStart,
    RoleLinks, RoleSession, RoleShared, TenantGate, start_owner,
};
use ultragraphdb_common_role::{
    ConnectionBound, OpenRequests, ROLE_ITERATIONS, RoleCluster, RoleCredentials, Server,
    finish_open, listen, serve_connection,
};
use ultragraphdb_common_service::{Readiness, RoleStart, Stopping};
use ultragraphdb_common_service_config::{EndpointSecurity, EndpointTable, ServiceConfig};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::{AUTHENTICATION_TIMEOUT, Endpoint};
use ultragraphdb_protocol_transport_compio::{AcceptPace, CompioTlsStream, Listener};
use ultragraphdb_transport_tls::{CertificateChainFile, PrivateKeyFile, TlsAcceptor};

/// How often the role looks at whether its owner stopped or the service was told to.
const OWNER_POLL: Duration = Duration::from_millis(20);

/// How long a stop waits on the open requests: within the 60 s a stopping role has, so a peer reading no answer costs no failure.
const FINISH_LIMIT: Duration = Duration::from_secs(50);

/// How long a compute process goes unanswering before the frontend takes it for hung, when `[storage] scan_stall_ms` does not say
/// (`PROTOCOL.md` §11.9).
const SCAN_STALL_MS_DEFAULT: u64 = 30_000;

/// The connections the endpoint holds when `[endpoint] connections_max` does not say (`PROTOCOL.md` §11.9).
const CONNECTIONS_MAX_DEFAULT: u32 = 4_096;

/// The frontend role: it recovers its owner, then listens at `[frontend] role_address` for the roles joining and at
/// `[endpoint] address`, when set, for clients, over TLS 1.3 or clear text as the table says. It is ready once it has connected
/// back to a partition, and runs until its owner stops or the service does, when it stops accepting connections and finishes its
/// open requests.
///
/// # Errors
///
/// [`RoleError::configuration`] for a setting absent or out of its bounds, a refused role secret, or a client endpoint deciding
/// neither TLS nor clear text; [`RoleError::unavailable`] when it cannot listen; what recovery refused or the owner stopped with.
///
/// A future dropped at the owner's start or at a listen leaves the owner, once started, running on its thread; one
/// dropped at its wait or in the drain leaves the accepting tasks and the requests still open running until their
/// runtime ends.
pub(crate) async fn frontend_role(
    start: RoleStart,
    readiness: Readiness,
    stopping: Stopping,
) -> Result<(), RoleError> {
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
    let unknown_secret = owner.unknown_secret();
    let (handle, mut stopped) = owner.into_parts();
    let open = OpenRequests::new(stopping.clone());
    let roles = Server::new(cluster.id(), "frontend", Endpoint::Frontend)?
        .with_iterations(ROLE_ITERATIONS)
        .with_open(open.clone());
    let links = RoleLinks::new(cluster.clone(), "frontend".to_string())
        .with_dial_ms(endpoint_limits(config).transaction_ms());
    if let Some(map) = handle.map().await {
        links.set_map(map);
    }
    let listening = readiness.clone();
    let shared = RoleShared::new(cluster.clone(), handle.clone(), links.clone(), readiness);
    let mut reporting = None;
    let mut accepting = vec![spawn(accept_roles(
        listen(role_address).await?,
        roles,
        credentials,
        shared,
    ))];
    if let Some(address) = config.endpoint().address() {
        let security = config
            .endpoint()
            .security()
            .map_err(|error| RoleError::configuration(error.to_string()))?;
        let core = EndpointCore::new(
            handle,
            links,
            TenantGate::new(EveryAuthenticated),
            endpoint_limits(config),
        )
        .with_open(open.clone());
        let clients =
            client_server(cluster.id(), config.endpoint(), unknown_secret)?.with_open(open.clone());
        let listener = listen(address).await?;
        reporting = Some(core.clone());
        accepting.push(spawn(accept_clients(listener, clients, core, security)));
    }
    listening.set_listening();
    // What the owner made durable it answered for, so a stop leaves nothing to flush but the answers on their way.
    while !stopping.is_stopping() {
        // The phases of the writes the endpoint relays, logged as they fall due.
        reporting.iter().for_each(EndpointCore::report_phases);
        match stopped.try_recv() {
            Ok(Some(outcome)) => return outcome,
            Ok(None) => compio::time::sleep(OWNER_POLL).await,
            Err(_) => {
                return Err(RoleError::software(
                    "the owner's thread ended without a word",
                ));
            }
        }
    }
    let _left = finish_open(accepting, &open, FINISH_LIMIT).await;
    Ok(())
}

/// What bounds a client connection, as the settings say.
fn endpoint_limits(config: &ServiceConfig) -> EndpointLimits {
    let held = config
        .endpoint()
        .held_bytes()
        .unwrap_or(EndpointTable::HELD_BYTES_DEFAULT);
    let stall = config
        .storage()
        .scan_stall_ms()
        .unwrap_or(SCAN_STALL_MS_DEFAULT);
    let limits = EndpointLimits::new(held, Duration::from_millis(stall));
    match config.endpoint().refusal_interval_ms() {
        Some(interval) => limits.with_refusal_interval(Duration::from_millis(interval)),
        None => limits,
    }
}

/// The server clients connect to, answering unknown names from the secret the metadata group holds, and past its bound of connections
/// with the limit error.
fn client_server(
    cluster: u64,
    endpoint: &EndpointTable,
    unknown_secret: [u8; 32],
) -> Result<Server, RoleError> {
    let bound = endpoint
        .connections_max()
        .unwrap_or(CONNECTIONS_MAX_DEFAULT);
    Ok(Server::new(cluster, "frontend", Endpoint::Client)?
        .with_unknown_secret(unknown_secret)
        .with_bound(ConnectionBound::new(u64::from(bound))))
}

fn required<'a>(value: Option<&'a str>, setting: &str) -> Result<&'a str, RoleError> {
    value.ok_or_else(|| RoleError::configuration(format!("the setting {setting} is required")))
}

/// What the settings say the owner is, each bound the state machines' own.
fn owner_start(config: &ServiceConfig) -> Result<OwnerStart, RoleError> {
    // A closure takes no type parameter, so it takes each error as a trait object (Ultra rules 1 and 9).
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
    if let Some(ms) = table.catalog_check_ms() {
        metadata = metadata
            .with_catalog_check_ms(ms)
            .map_err(|error| refused(&error))?;
    }
    let owner = OwnerStart::new(metadata, sequencer, partition_count);
    Ok(match table.bootstrap_secret_file() {
        Some(file) => owner.with_bootstrap_file(file),
        None => owner,
    })
}

/// Accepts the roles joining, each connection served in a task of its own. A future dropped at an accept, as its
/// runtime's end drops it, takes no connection half-way.
async fn accept_roles(
    listener: Listener,
    server: Server,
    credentials: RoleCredentials,
    shared: RoleShared,
) {
    let mut pace = AcceptPace::default();
    // An event loop: it accepts for as long as its runtime runs, which ends with the service (Ultra rule 2).
    loop {
        let carrier = pace.accept("role", || listener.accept()).await;
        let (server, credentials) = (server.clone(), credentials.clone());
        let session = RoleSession::new(shared.clone());
        spawn(async move {
            serve_connection(carrier, &server, credentials, session).await;
        })
        .detach();
    }
}

/// Accepts clients, each connection served, its TLS handshake included, in a task of its own. A future dropped at an
/// accept, as its runtime's end drops it, takes no connection half-way.
async fn accept_clients(
    listener: Listener,
    server: Server,
    core: EndpointCore<EveryAuthenticated>,
    security: EndpointSecurity,
) {
    let acceptor = match &security {
        EndpointSecurity::Plaintext => None,
        EndpointSecurity::Tls {
            cert_file,
            key_file,
        } => {
            let chain = CertificateChainFile::new(cert_file);
            let key = PrivateKeyFile::new(key_file);
            match TlsAcceptor::from_pem_files(&chain, &key) {
                Ok(acceptor) => Some(Rc::new(acceptor)),
                Err(error) => {
                    tracing::error!(%error, "the endpoint's certificate could not be read");
                    return;
                }
            }
        }
    };
    let mut pace = AcceptPace::default();
    // An event loop: it accepts for as long as its runtime runs, which ends with the service (Ultra rule 2).
    loop {
        let carrier = pace.accept("client", || listener.accept()).await;
        let server = server.clone();
        let credentials = AskedCredentials::default();
        let session = ClientSession::new(core.clone(), credentials.clone(), carrier.peer_ip());
        let acceptor = acceptor.clone();
        spawn(async move {
            let Some(acceptor) = acceptor else {
                return serve_connection(carrier, &server, credentials, session).await;
            };
            // A handshake counts against the bound and ends by the authentication timeout, as an unauthenticated
            // connection does (`PROTOCOL.md` §11.9): a peer that sends nothing holds no descriptor past it.
            let handshaking = server.bound().map(ConnectionBound::enter);
            if handshaking.as_ref().is_some_and(|held| !held.within()) {
                return tracing::debug!("a TLS handshake past the connection bound was dropped");
            }
            let accepted = compio::time::timeout(
                AUTHENTICATION_TIMEOUT,
                CompioTlsStream::accept(&acceptor, carrier),
            )
            .await;
            drop(handshaking);
            match accepted {
                Ok(Ok(stream)) => {
                    serve_connection(stream, &server, credentials, session).await;
                }
                // A clear-text Hello reaching this endpoint fails here, and no frame is read.
                Ok(Err(error)) => tracing::debug!(%error, "a TLS handshake failed"),
                Err(_) => tracing::debug!("a TLS handshake did not end in time"),
            }
        })
        .detach();
    }
}
