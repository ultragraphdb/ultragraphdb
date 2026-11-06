/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use compio::runtime::spawn;
use ultragraphdb_common_compute::{
    ComputeCore, ComputeSession, CoreListener, ReadTimes, read_times_of,
};
use ultragraphdb_common_role::{
    COMPUTE_WELCOME_CREDIT, OpenRequests, ROLE_ITERATIONS, RoleClient, RoleCluster,
    RoleCredentials, Server, finish_open, frontend_address, retry_until, serve_connection,
};
use ultragraphdb_common_service::{Readiness, RoleKind, RoleStart, Stopping, core_runtime};
use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::{Address, Endpoint, TransportError};
use ultragraphdb_protocol_wire::{BodyView, Join, Members, MembersView};

/// How often the role looks at its peers and whether it was told to stop, and dials a peer it lost.
const POLL: Duration = Duration::from_millis(100);

/// How long a stop waits on the reads under way: within the 60 s a stopping role has, as the frontend's.
const FINISH_LIMIT: Duration = Duration::from_secs(50);

/// How many times per `cluster.join_deadline_ms` compute asks each peer whether it still answers.
const ASKS_PER_DEADLINE: u32 = 2;

/// The compute role: one runtime per core, each accepting at `[compute] role_address` and running what arrives on it wholly
/// there. It joins the frontend and is ready once the members it reads name a partition that answers. It then asks the frontend
/// and each of those partitions whether they answer, and a peer that does not, by the deadline, ends it.
///
/// # Errors
///
/// [`RoleError::configuration`] for a setting absent or a refused role secret; [`RoleError::unavailable`] when it
/// cannot listen, has not joined by its deadline, or lost a peer past it.
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
    let listener = CoreListener::bind(own).await?;
    // Every core's reads, counted until each ends: a stop refuses new ones and drains these (role-binaries "Readiness, health and
    // shutdown").
    let open = OpenRequests::new(stopping.clone());
    let serving = CoreServing {
        cluster: cluster.clone(),
        frontend: frontend.clone(),
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
    let name = kind.to_string();
    let joined = retry_until(&cluster, &kind.to_string(), stopping, || {
        attempt(&frontend, &cluster, &name, join)
    })
    .await?;
    let Some(partitions) = joined else {
        return Ok(());
    };
    readiness.set(true);
    let peers = std::iter::once(frontend.clone())
        .chain(partitions)
        .collect();
    hold_peers(peers, &cluster, stopping).await?;
    let _left = finish_open(
        Vec::<compio::runtime::JoinHandle<()>>::new(),
        &open,
        FINISH_LIMIT,
    )
    .await;
    Ok(())
}

/// What each core accepts and reads with.
#[derive(Debug, Clone)]
struct CoreServing {
    cluster: RoleCluster,
    frontend: Address,
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
async fn accept_on_core(listener: CoreListener, serving: CoreServing) {
    let listener = match listener.on_this_core() {
        Ok(listener) => listener,
        Err(error) => {
            tracing::error!(%error, "a core does not listen");
            return;
        }
    };
    let name = RoleKind::Compute.to_string();
    let core = ComputeCore::new(serving.cluster, serving.frontend, serving.times, &name);
    loop {
        match listener.accept().await {
            Ok(carrier) => {
                let (server, credentials) = (serving.server.clone(), serving.credentials.clone());
                let session = ComputeSession::new(core.clone()).with_open(serving.open.clone());
                spawn(async move {
                    serve_connection(carrier, &server, credentials, session).await;
                })
                .detach();
            }
            Err(error) => tracing::warn!(%error, "a role connection was not accepted"),
        }
    }
}

/// Asks each of `peers` until the service stops or one is lost past the deadline.
async fn hold_peers(
    peers: Vec<Address>,
    cluster: &RoleCluster,
    stopping: &Stopping,
) -> Result<(), RoleError> {
    // Shared with this runtime's tasks alone, each of which fills it at most once.
    let lost: Rc<Cell<Option<RoleError>>> = Rc::default();
    for peer in peers {
        let (cluster, stopping, lost) = (cluster.clone(), stopping.clone(), Rc::clone(&lost));
        spawn(async move {
            if let Err(failure) = hold(&peer, &cluster, &stopping).await {
                lost.set(Some(failure));
            }
        })
        .detach();
    }
    while !stopping.is_stopping() {
        if let Some(failure) = lost.take() {
            return Err(failure);
        }
        compio::time::sleep(POLL).await;
    }
    Ok(())
}

/// Asks whether `peer` answers, for as long as it does. A held connection cannot tell: a peer stopped or its host gone closes
/// nothing, a role connection has no idle limit and the protocol no ping (`PROTOCOL.md` §11.4, §11.12), so a read would wait
/// forever. A dial is a question it must answer, by its Welcome and its proof of the secret.
async fn hold(peer: &Address, cluster: &RoleCluster, stopping: &Stopping) -> Result<(), RoleError> {
    let every = (cluster.deadline() / ASKS_PER_DEADLINE).max(POLL);
    while answers(peer, cluster, stopping).await? {
        compio::time::sleep(every).await;
    }
    Ok(())
}

/// Dials `peer` until it answers, each dial limited to the time left before the deadline, past which it is lost; false once the
/// service stops.
async fn answers(
    peer: &Address,
    cluster: &RoleCluster,
    stopping: &Stopping,
) -> Result<bool, RoleError> {
    let name = RoleKind::Compute.to_string();
    let due = now() + cluster.deadline();
    while !stopping.is_stopping() {
        let left = due.saturating_duration_since(now()).max(POLL);
        let dial = RoleClient::dial(peer, cluster.id(), &name, cluster.secret().password());
        let why = match compio::time::timeout(left, dial).await {
            Ok(Ok(_client)) => return Ok(true),
            Ok(Err(error)) => error.to_string(),
            Err(_) => format!("a dial had no answer within {} ms", left.as_millis()),
        };
        if now() >= due {
            return Err(RoleError::unavailable(format!(
                "compute lost the role at {peer}, which did not return within {} ms: {why}. Start that role again, then compute",
                cluster.deadline().as_millis()
            )));
        }
        tracing::warn!(%peer, %why, "a role did not answer; asking it again");
        compio::time::sleep(POLL).await;
    }
    Ok(false)
}

#[expect(
    clippy::disallowed_methods,
    reason = "a deadline in the shell, which no simulation replays"
)]
fn now() -> Instant {
    Instant::now()
}

/// One attempt: dial, authenticate, Join, read the Members until a partition has joined: where those partitions listen.
async fn attempt(
    frontend: &Address,
    cluster: &RoleCluster,
    name: &str,
    join: &Join,
) -> Result<Vec<Address>, TransportError> {
    let mut client =
        RoleClient::dial(frontend, cluster.id(), name, cluster.secret().password()).await?;
    drop(client.request(join).await?);
    let answer = client.request(&Members::new()).await?;
    let view = answer.view().map_err(TransportError::protocol)?;
    let members = MembersView::from_frame(&view).map_err(TransportError::protocol)?;
    let partitions = (0..members.len())
        .filter_map(|index| members.member(index))
        .filter(|member| member.partition() > 0)
        .filter_map(|member| members.address(member.address() as usize))
        .map(Address::parse)
        .collect::<Result<Vec<_>, _>>()?;
    let Some(first) = partitions.first() else {
        return Err(TransportError::authentication(
            "no partition has joined yet",
        ));
    };
    drop(RoleClient::dial(first, cluster.id(), name, cluster.secret().password()).await?);
    Ok(partitions)
}
