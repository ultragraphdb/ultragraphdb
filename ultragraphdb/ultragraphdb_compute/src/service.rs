/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::{Duration, Instant};

use compio::runtime::spawn;
use ultragraphdb_common_compute::{
    ComputeCore, ComputeSession, ReadTimes, hold_peers, join_group, read_times_of,
};
use ultragraphdb_common_role::{
    COMPUTE_WELCOME_CREDIT, OpenRequests, ROLE_ITERATIONS, RoleCluster, RoleCredentials, Server,
    finish_open, serve_connection, system_address,
};
use ultragraphdb_common_service::{
    CoreListener, Readiness, RoleKind, RoleStart, Stopping, core_runtime,
};
use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::{Address, Endpoint};
use ultragraphdb_protocol_transport_compio::AcceptPace;
use ultragraphdb_protocol_wire::Join;

/// How long a stop waits on the reads under way: within the 60 s a stopping role has, as the frontend's.
const FINISH_LIMIT: Duration = Duration::from_secs(50);

/// The compute role: one runtime per core, each accepting at `[compute] role_address` and running what arrives on it wholly
/// there. It joins the system group at `cluster.system_address` ([`system_address`]) and is ready once the members it reads name a
/// partition that answers. It then asks the group and each of those partitions whether they answer, joining the group again
/// when it no longer lists compute, the group asked again at once when its connection closes, and a peer that does not
/// answer by the deadline ends it.
///
/// # Errors
///
/// [`RoleError::configuration`] for a setting absent or a refused role secret; [`RoleError::unavailable`] when it
/// cannot listen, has not joined by its deadline, or lost a peer past it.
///
/// A future dropped at its await leaves what `join_as` leaves.
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

/// Joins at `own`, serves on every core, holds the peers until the service stops or one is lost, then drains the reads
/// still open. A future dropped at any await leaves the cores accepting, their tasks detached on their own runtimes, and
/// readiness as it was set; dropped in the drain, it leaves the reads still open to their runtimes' end.
async fn join_as(
    endpoint: Endpoint,
    start: &RoleStart,
    own: &str,
    join: &Join,
    readiness: &Readiness,
    stopping: &Stopping,
) -> Result<(), RoleError> {
    let cluster = RoleCluster::read(start)?;
    let group = system_address(start)?;
    let listener = CoreListener::bind(own).await?;
    // Every core's reads, counted until each ends: a stop refuses new ones and drains these (role-binaries "Readiness, health and
    // shutdown").
    let open = OpenRequests::new(stopping.clone());
    let serving = CoreServing {
        cluster: cluster.clone(),
        group: group.clone(),
        credentials: cluster.secret().credentials()?,
        server: Server::new(cluster.id(), "role", endpoint)?
            .with_welcome(|welcome| welcome.with_initial_credit(COMPUTE_WELCOME_CREDIT))
            .with_iterations(ROLE_ITERATIONS)
            .with_open(open.clone()),
        open: open.clone(),
        times: read_times_of(
            start.config().storage().scan_stall_ms(),
            start.config().storage().read_retain_ms(),
        ),
    };
    start_cores(start.config(), &listener, &serving)?;
    spawn(accept_on_core(listener, serving)).detach();
    let Some(partitions) = join_group(&group, join, &cluster, stopping).await? else {
        return Ok(());
    };
    readiness.set(true);
    let origin = now();
    let clock = move || now().saturating_duration_since(origin);
    let held = hold_peers(&group, join, partitions, &cluster, stopping, clock).await;
    // A lost peer stops the role through the path a SIGTERM takes (role-binaries, "A lost dependency exits with its code";
    // system-group, "A lost system group is waited for until the deadline"): no core takes a new read, and the reads still
    // open are drained, before the process exits with code 69.
    if let Err(failure) = &held {
        tracing::error!(%failure, "a peer is gone; stopping");
        tracing::info!(role = %RoleKind::Compute, "stopping");
        stopping.set();
    }
    let _left = finish_open(
        Vec::<compio::runtime::JoinHandle<()>>::new(),
        &open,
        FINISH_LIMIT,
    )
    .await;
    held
}

/// What each core accepts and reads with.
#[derive(Debug, Clone)]
struct CoreServing {
    cluster: RoleCluster,
    group: Address,
    credentials: RoleCredentials,
    server: Server,
    open: OpenRequests,
    times: ReadTimes,
}

/// A thread and a runtime for every core but this one, each serving a handle of `listener`.
fn start_cores(
    config: &ServiceConfig,
    listener: &CoreListener,
    serving: &CoreServing,
) -> Result<(), RoleError> {
    let cores = std::thread::available_parallelism().map_or(1, usize::from);
    for core in 1..cores {
        let (config, listener, serving) = (config.clone(), listener.another()?, serving.clone());
        #[expect(
            clippy::disallowed_methods,
            reason = "a core is a real thread, which no simulation replays"
        )]
        let spawned = std::thread::Builder::new()
            .name(format!("compute-{core}"))
            .spawn(move || {
                let served = core_runtime(&config, core).map(|(runtime, _cpu)| {
                    runtime.block_on(accept_on_core(listener, serving));
                });
                if let Err(error) = served {
                    tracing::error!(core, %error, "a core did not start");
                }
            });
        drop(spawned.map_err(|error| RoleError::unavailable(format!("a core's thread: {error}")))?);
    }
    Ok(())
}

/// Accepts on this core, whose connections and reads stay on it.
///
/// A future dropped at an accept, as its runtime's end drops it, takes no connection half-way: each one accepted runs
/// in a task of its own.
async fn accept_on_core(listener: CoreListener, serving: CoreServing) {
    let listener = match listener.on_this_core() {
        Ok(listener) => listener,
        Err(error) => {
            tracing::error!(%error, "a core does not listen");
            return;
        }
    };
    let name = RoleKind::Compute.to_string();
    let core = ComputeCore::new(serving.cluster, serving.group, serving.times, &name);
    let mut pace = AcceptPace::default();
    // An event loop: it accepts for as long as its runtime runs, which ends with the service (Ultra rule 2).
    loop {
        let carrier = pace.accept("role", || listener.accept()).await;
        let (server, credentials) = (serving.server.clone(), serving.credentials.clone());
        let session = ComputeSession::new(core.clone()).with_open(serving.open.clone());
        spawn(async move {
            serve_connection(carrier, &server, credentials, session).await;
        })
        .detach();
    }
}

/// The time now, which the peers' deadlines are read by.
#[expect(
    clippy::disallowed_methods,
    reason = "a deadline in the shell, which no simulation replays"
)]
fn now() -> Instant {
    Instant::now()
}
