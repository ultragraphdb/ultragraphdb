/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::rc::Rc;
use std::time::Duration;

use compio::runtime::spawn;
use ultragraphdb_common_compute::{
    AuthThrottle, ClientSession, ComputeCore, EndpointCore, EndpointLimits, EveryAuthenticated,
    LoginLimits, ReadTimes, RelayAuthenticator, RoleLinks, SystemLink, TenantGate, ThrottleHandle,
    read_times_of,
};
use ultragraphdb_common_role::{
    ConnectionBound, OpenRequests, RoleCluster, Server, finish_open, serve_connection_with,
    system_address,
};
use ultragraphdb_common_service::{
    CoreListener, Readiness, RoleKind, RoleStart, Stopping, core_runtime,
};
use ultragraphdb_common_service_config::{EndpointSecurity, EndpointTable, ServiceConfig};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::{AUTHENTICATION_TIMEOUT, Address, Endpoint};
use ultragraphdb_protocol_transport_compio::{AcceptPace, CompioTlsStream, Listener};
use ultragraphdb_transport_tls::{CertificateChainFile, PrivateKeyFile, TlsAcceptor};

/// How often the role looks at whether the system group is gone or the service was told to stop.
const OWNER_POLL: Duration = Duration::from_millis(20);

/// How long a stop waits on the open requests: within the 60 s a stopping role has, so a peer reading no answer costs no failure.
const FINISH_LIMIT: Duration = Duration::from_secs(50);

/// How long a partition may stall a Scan, when `[storage] scan_stall_ms` does not say (`PROTOCOL.md` §11.9).
const SCAN_STALL_MS_DEFAULT: u64 = 30_000;

/// The connections the endpoint holds when `[endpoint] connections_max` does not say (`PROTOCOL.md` §11.9).
const CONNECTIONS_MAX_DEFAULT: u32 = 4_096;

/// The frontend role: its endpoint cores dial the system group at `cluster.system_address` (design D5) and serve clients at
/// `[endpoint] address`, when set, over TLS 1.3 or clear text as the table says, on an endpoint core per CPU. It hosts no group
/// and listens at no role address. It is ready once its first core's view holds a close and a placement and Members lists every
/// partition, and runs until the service stops, or until the system group has been gone for `[cluster] join_deadline_ms`, when
/// every core stops accepting connections and the open requests are finished.
///
/// # Errors
///
/// [`RoleError::configuration`] for a setting absent or out of its bounds, a refused role secret, or a client endpoint deciding
/// neither TLS nor clear text; [`RoleError::unavailable`] when it cannot listen, or the system group was gone past the deadline.
///
/// A future dropped at a listen leaves nothing running; one dropped at its wait or in the drain leaves the accepting tasks and
/// the requests still open running until their runtime ends.
#[expect(
    clippy::disallowed_methods,
    reason = "a deadline in the shell, which no simulation replays"
)]
pub(crate) async fn frontend_role(
    start: RoleStart,
    readiness: Readiness,
    stopping: Stopping,
) -> Result<(), RoleError> {
    let cluster = RoleCluster::read(&start)?;
    let config = start.config();
    let open = OpenRequests::new(stopping.clone());
    let address = system_address(&start)?;
    let links = core_links(&cluster, config);
    let system = SystemLink::dial(
        address.clone(),
        cluster.clone(),
        links.clone(),
        endpoint_limits(config).transaction_ms(),
        readiness.clone(),
    );
    let clients = match config.endpoint().address() {
        Some(endpoint) => Some(Clients::bind(config, endpoint, cluster.id(), &open).await?),
        None => None,
    };
    if let Some(clients) = &clients {
        start_cores(config, &cluster, &address, clients, &stopping)?;
    }
    let mut accepting = Vec::new();
    let core = clients.as_ref().map(|clients| {
        let reads = core_reads(&cluster, &address, config);
        clients.core(system.clone(), links, (0, reads))
    });
    if let (Some(clients), Some(core)) = (clients, &core) {
        accepting.push(spawn(accept_clients(
            clients.listener.on_this_core()?,
            clients.server,
            core.clone(),
            clients.security,
        )));
    }
    readiness.set_listening();
    let deadline = cluster.deadline();
    let mut lost = None;
    while !stopping.is_stopping() {
        // The phases of the writes the endpoint relays and its Checks' round trips, logged as they fall due.
        core.iter().for_each(EndpointCore::report_phases);
        if system
            .lost_since()
            .is_some_and(|since| since.elapsed() >= deadline)
        {
            lost = Some(RoleError::unavailable(format!(
                "the system group at {address} has been gone for join_deadline_ms, {} ms",
                deadline.as_millis()
            )));
            break;
        }
        compio::time::sleep(OWNER_POLL).await;
    }
    // A lost group stops the role through the path a SIGTERM takes (system-group, "A lost system group is waited for until
    // the deadline"): every core stops accepting, and the open requests are finished, before the process exits with code 69.
    if let Some(failure) = &lost {
        tracing::error!(%failure, "the system group is gone; stopping");
        tracing::info!(role = %RoleKind::Frontend, "stopping");
        stopping.set();
    }
    let _left = finish_open(accepting, &open, FINISH_LIMIT).await;
    lost.map_or(Ok(()), Err)
}

/// What every endpoint core serves its clients with: its handle of the client socket, the server, the transport's security, the
/// limits, the count of open requests a stop waits on and the way to the throttle.
struct Clients {
    listener: CoreListener,
    server: Server,
    security: EndpointSecurity,
    limits: EndpointLimits,
    open: OpenRequests,
    logins: ThrottleHandle,
}

impl Clients {
    /// The endpoint at `address`, bound once for every core, and the throttle the first core holds for every core, run on this
    /// core's runtime (design D15).
    async fn bind(
        config: &ServiceConfig,
        address: &str,
        cluster: u64,
        open: &OpenRequests,
    ) -> Result<Self, RoleError> {
        let security = config
            .endpoint()
            .security()
            .map_err(|error| RoleError::configuration(error.to_string()))?;
        let limits = endpoint_limits(config);
        let (logins, throttling) = AuthThrottle::new(login_limits(config)).serve();
        spawn(throttling).detach();
        Ok(Self {
            listener: CoreListener::bind(address).await?,
            server: client_server(cluster, config.endpoint())?.with_open(open.clone()),
            security,
            limits,
            open: open.clone(),
            logins,
        })
    }

    /// The same endpoint for another core, on its own handle of the socket.
    fn another(&self) -> Result<Self, RoleError> {
        Ok(Self {
            listener: self.listener.another()?,
            server: self.server.clone(),
            security: self.security.clone(),
            limits: self.limits,
            open: self.open.clone(),
            logins: self.logins.clone(),
        })
    }

    /// Endpoint core `core`, asking the system group through `system`, reaching the roles through `links` and running its
    /// clients' reads over `reads`, its own connections to the partitions (design D3).
    fn core(
        &self,
        system: SystemLink,
        links: RoleLinks,
        (core, reads): (usize, Rc<ComputeCore>),
    ) -> EndpointCore<EveryAuthenticated> {
        EndpointCore::new(
            system.with_core(core),
            self.logins.clone(),
            links,
            TenantGate::new(EveryAuthenticated),
            self.limits,
        )
        .with_open(self.open.clone())
        .with_core(core)
        .with_compute(reads)
    }
}

/// What an endpoint core past the first starts on its thread with.
struct CoreStart {
    config: ServiceConfig,
    cluster: RoleCluster,
    /// The system group's role address, which the core dials.
    system: Address,
    clients: Clients,
    stopping: Stopping,
    core: usize,
}

/// A core's connections to the partitions its clients' reads run over, dialling as the frontend: the placement map and where
/// each partition listens asked at the system group's role address `system`, and no Join sent (`PROTOCOL.md` §11.12, *Reads*).
fn core_reads(cluster: &RoleCluster, system: &Address, config: &ServiceConfig) -> Rc<ComputeCore> {
    let times: ReadTimes = read_times_of(
        config.storage().scan_stall_ms(),
        config.storage().read_retain_ms(),
    );
    ComputeCore::new(cluster.clone(), system.clone(), times, "frontend")
}

/// A core's links to the other roles, dialling as the frontend; the placement map is its link's to read.
fn core_links(cluster: &RoleCluster, config: &ServiceConfig) -> RoleLinks {
    RoleLinks::new(cluster.clone(), "frontend".to_string())
        .with_dial_ms(endpoint_limits(config).transaction_ms())
}

/// Starts an endpoint core on each CPU the process may use but the first, the service's own.
fn start_cores(
    config: &ServiceConfig,
    cluster: &RoleCluster,
    system: &Address,
    clients: &Clients,
    stopping: &Stopping,
) -> Result<(), RoleError> {
    for core in 1..std::thread::available_parallelism().map_or(1, usize::from) {
        let start = CoreStart {
            config: config.clone(),
            cluster: cluster.clone(),
            system: system.clone(),
            clients: clients.another()?,
            stopping: stopping.clone(),
            core,
        };
        #[expect(
            clippy::disallowed_methods,
            reason = "a core is a real thread, which no simulation replays"
        )]
        let spawned = std::thread::Builder::new()
            .name(format!("frontend-{core}"))
            .spawn(move || {
                let served = core_runtime(&start.config, core).map(|(runtime, _cpu)| {
                    runtime.block_on(serve_core(start));
                });
                if let Err(error) = served {
                    tracing::error!(core, %error, "an endpoint core did not start");
                }
            });
        drop(spawned.map_err(|error| RoleError::unavailable(format!("a core's thread: {error}")))?);
    }
    Ok(())
}

/// Serves clients on this core until the service stops, then stops accepting and waits for the open requests, as core 0 does.
///
/// A future dropped at a wait, as its runtime's end drops it, leaves the core's connections to that end.
async fn serve_core(start: CoreStart) {
    let CoreStart {
        config,
        cluster,
        system,
        clients,
        stopping,
        core,
    } = start;
    let links = core_links(&cluster, &config);
    let reads = core_reads(&cluster, &system, &config);
    let link = SystemLink::dial(
        system,
        cluster,
        links.clone(),
        endpoint_limits(&config).transaction_ms(),
        Readiness::default(),
    );
    let endpoint = clients.core(link, links, (core, reads));
    let listener = match clients.listener.on_this_core() {
        Ok(listener) => listener,
        Err(error) => return tracing::error!(core, %error, "an endpoint core does not listen"),
    };
    let accepting = spawn(accept_clients(
        listener,
        clients.server,
        endpoint.clone(),
        clients.security,
    ));
    while !stopping.is_stopping() {
        // The phases of the writes the core relays, logged as they fall due.
        endpoint.report_phases();
        compio::time::sleep(OWNER_POLL).await;
    }
    let _left = finish_open(vec![accepting], &clients.open, FINISH_LIMIT).await;
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
    EndpointLimits::new(held, Duration::from_millis(stall))
}

/// When a source's failed authentications wait and when they ban it, as the settings say (design D15); the settings' check
/// refuses 0 for each.
fn login_limits(config: &ServiceConfig) -> LoginLimits {
    let endpoint = config.endpoint();
    LoginLimits::new(
        endpoint
            .auth_failures_before_throttle()
            .unwrap_or(LoginLimits::BEFORE_THROTTLE_DEFAULT),
        endpoint
            .auth_throttled_failures_before_ban()
            .unwrap_or(LoginLimits::THROTTLED_BEFORE_BAN_DEFAULT),
        endpoint
            .auth_ban_ms()
            .map_or(LoginLimits::BAN_DEFAULT, Duration::from_millis),
    )
}

/// The server clients connect to, each exchange relayed to the system group (design D8), and past its bound of connections with
/// the limit error.
fn client_server(cluster: u64, endpoint: &EndpointTable) -> Result<Server, RoleError> {
    let bound = endpoint
        .connections_max()
        .unwrap_or(CONNECTIONS_MAX_DEFAULT);
    Ok(Server::new(cluster, "frontend", Endpoint::Client)?
        .with_bound(ConnectionBound::new(u64::from(bound))))
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
        let relay = RelayAuthenticator::default();
        let peer = carrier.peer_ip();
        let session = ClientSession::new(core.clone(), relay.clone(), peer);
        let (acceptor, core) = (acceptor.clone(), core.clone());
        spawn(async move {
            // A banned source's connection closes as accepted, before TLS or any frame (design D15); asked on the connection's own
            // task, so accepting never waits on the owner.
            if !core.admits(peer).await {
                return tracing::debug!(?peer, "a banned source's connection was closed");
            }
            let Some(acceptor) = acceptor else {
                return serve_connection_with(carrier, &server, relay, session).await;
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
                    serve_connection_with(stream, &server, relay, session).await;
                }
                // A clear-text Hello reaching this endpoint fails here, and no frame is read.
                Ok(Err(error)) => tracing::debug!(%error, "a TLS handshake failed"),
                Err(_) => tracing::debug!("a TLS handshake did not end in time"),
            }
        })
        .detach();
    }
}
