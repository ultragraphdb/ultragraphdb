/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A played system group and played partitions on the test's runtime, and compute's core beside them.

use std::future::Future;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_compute::{ComputeCore, ReadExecutor, ReadTimes, RoleLinks};
use ultragraphdb_common_role::{
    Answer, OpenRequests, PARTITION_INITIAL_CREDIT, ROLE_ITERATIONS, RoleCluster, RoleSecret,
    Server, Session, serve_connection,
};
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_protocol_transport::{Address, Endpoint, ReceivedFrame};
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BucketRange, Command, DEFAULT_TRANSACTION_MS, ErrorCode, ErrorReply, FLAG_LAST,
    Header, Join, Members, Placement, PreferredLeader,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use super::apart_partition::PlayedPartition;
use super::apart_session::ReadSession;
use super::apart_tap::ApartTap;

/// The played cluster's identity.
pub const CLUSTER: u64 = 7;

/// How long one test's work may take in all.
const TEST_WAIT: Duration = Duration::from_secs(30);

/// The secret files written, so each cluster writes its own.
static SECRETS: AtomicU64 = AtomicU64::new(0);

/// Runs `work` on a runtime of its own, failing past [`TEST_WAIT`].
pub fn run<T>(work: impl Future<Output = TestResult<T>>) -> TestResult<T> {
    Runtime::new()?.block_on(async {
        compio::time::timeout(TEST_WAIT, work)
            .await
            .map_err(|_| TestError::new(format!("the test did not end within {TEST_WAIT:?}")))?
    })
}

/// The played cluster's identity and a role secret of its own, for a role double a test starts beside it.
pub fn played_role_cluster() -> TestResult<RoleCluster> {
    Ok(RoleCluster::new(
        CLUSTER,
        RoleSecret::read(&secret_file()?)?,
        Duration::from_secs(5),
    ))
}

/// Times whose stall is `scan_stall`, holding no version a test does not ask for within a minute.
pub fn times(scan_stall: Duration) -> ReadTimes {
    ReadTimes::new(scan_stall, Duration::from_secs(600))
}

/// A role secret file of the cluster's own.
fn secret_file() -> TestResult<String> {
    let written = SECRETS.fetch_add(1, Ordering::Relaxed);
    let file = std::env::var_os("TEST_TMPDIR")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join(format!(
            "compute-apart-secret-{}-{written}",
            std::process::id()
        ));
    std::fs::write(&file, "the apart cluster's role secret")?;
    Ok(file.display().to_string())
}

/// `body` as the reply on `stream`.
fn reply_of<B: BodyBuilder>(body: &B, stream: u64) -> Answer {
    let header = Header::new(CLUSTER, B::COMMAND)
        .with_stream(stream)
        .with_flags(FLAG_LAST);
    match body.frame(header) {
        Ok(frame) => Answer::Reply(frame),
        Err(rule) => Answer::Refuse(ErrorReply::protocol(&rule)),
    }
}

/// One connection at the played system group: the placement and where each partition listens.
#[derive(Debug)]
struct SystemConnection {
    placement: Rc<Placement>,
    members: Rc<Members>,
}

impl Session for SystemConnection {
    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        let command = frame.view().map(|view| view.command());
        match command {
            Ok(Command::Placement) => reply_of(self.placement.as_ref(), stream),
            Ok(Command::Members) => reply_of(self.members.as_ref(), stream),
            Ok(other) => Answer::Refuse(ErrorReply::new(
                ErrorCode::Unavailable,
                format!("command {} is not played", other.number()),
            )),
            Err(rule) => Answer::Refuse(ErrorReply::protocol(&rule)),
        }
    }
}

/// The even placement over `partitions`, as the system group's first start forms it.
pub fn even_map(partitions: u32) -> TestResult<PlacementMap> {
    Ok(PlacementMap::even(partitions, NodeId::new(1))?)
}

/// A core's links of `cluster`, dialling as compute: the placement map `map` set, and each partition of `listed` at the address
/// it is listed at, as the core's system link learns them from Placement and Members.
pub fn links_listing(
    cluster: &RoleCluster,
    map: PlacementMap,
    listed: &[(u32, String)],
) -> RoleLinks {
    let links = RoleLinks::new(cluster.clone(), "compute".to_string());
    links.set_map(map);
    for (partition, address) in listed {
        links.joined(&Join::storage(address.clone(), vec![*partition]));
    }
    links
}

/// How long a test waits for a core's links to let the streams of ended reads go, a dropped task's included, and how often it
/// looks.
const STREAMS_WAIT: Duration = Duration::from_secs(2);
const STREAMS_LOOK: Duration = Duration::from_millis(10);

/// The links `core` keeps to partitions and how many of them still hold a stream, once the streams of reads that ended have
/// gone or past [`STREAMS_WAIT`].
pub async fn links_holding_streams(core: &ComputeCore) -> (usize, usize) {
    let gone = async {
        while core.links_held().1 > 0 {
            compio::time::sleep(STREAMS_LOOK).await;
        }
    };
    // Past the wait, what is still held is the answer.
    let _waited = compio::time::timeout(STREAMS_WAIT, gone).await;
    core.links_held()
}

/// A cluster played on the test's runtime.
#[derive(Debug)]
pub struct ApartCluster {
    cluster: RoleCluster,
    system: Address,
    partitions: Vec<Rc<PlayedPartition>>,
    /// Where compute dials each partition, by number: its own address, or its tap's.
    listed: Vec<(u32, Address)>,
    taps: Vec<Rc<ApartTap>>,
    map: PlacementMap,
}

impl ApartCluster {
    /// A system group and `partitions` partitions, placed evenly, served on this runtime.
    pub async fn start(partitions: u32) -> TestResult<Self> {
        let order: Vec<u32> = (1..=partitions).collect();
        Self::start_in(&order, false).await
    }

    /// A system group and a partition for each number of `order`, placed evenly over them, each bound to its port in the
    /// order `order` lists them, and listed by number; compute dials each through a tap of its own when `tapped`.
    pub async fn start_in(order: &[u32], tapped: bool) -> TestResult<Self> {
        let cluster = RoleCluster::new(
            CLUSTER,
            RoleSecret::read(&secret_file()?)?,
            Duration::from_secs(5),
        );
        let partitions = u32::try_from(order.len())?;
        let mut bound: Vec<(u32, Rc<PlayedPartition>, Address)> = Vec::new();
        for partition in order {
            let held = PlayedPartition::new(*partition);
            let server = Server::new(CLUSTER, "storage", Endpoint::Partition)?
                .with_welcome(|welcome| welcome.with_initial_credit(PARTITION_INITIAL_CREDIT));
            let serving = Rc::clone(&held);
            let address = serve(&cluster, server, move || serving.connection()).await?;
            bound.push((*partition, held, address));
        }
        bound.sort_by_key(|(partition, _, _)| *partition);
        let mut members = Members::new();
        let (mut played, mut listed, mut taps) = (Vec::new(), Vec::new(), Vec::new());
        for (partition, held, address) in bound {
            let dialled = if tapped {
                let tap = ApartTap::start(&address).await?;
                let at = tap.address().clone();
                taps.push(tap);
                at
            } else {
                address
            };
            members.push_storage(partition, dialled.to_string());
            listed.push((partition, dialled));
            played.push(held);
        }
        let map = even_map(partitions)?;
        let mut placement = Placement::reply(0, 0, 1, 0);
        for (range, partition) in map.ranges_at(Epoch::ZERO) {
            placement.push_range(BucketRange::new(
                range.start(),
                range.end(),
                partition.get(),
            ));
        }
        (1..=partitions)
            .for_each(|partition| placement.push_leader(PreferredLeader::new(partition, 1)));
        let (placement, members) = (Rc::new(placement), Rc::new(members));
        let server = Server::new(CLUSTER, "compute", Endpoint::System)?;
        let system = serve(&cluster, server, move || SystemConnection {
            placement: Rc::clone(&placement),
            members: Rc::clone(&members),
        })
        .await?;
        Ok(Self {
            cluster,
            system,
            partitions: played,
            listed,
            taps,
            map,
        })
    }

    /// Played partition `partition`.
    pub fn partition(&self, partition: u32) -> TestResult<&Rc<PlayedPartition>> {
        self.partitions
            .get((partition as usize).wrapping_sub(1))
            .context("a played partition")
    }

    /// The tap in front of partition `partition`, in a cluster started tapped.
    pub fn tap(&self, partition: u32) -> TestResult<&Rc<ApartTap>> {
        self.taps
            .get((partition as usize).wrapping_sub(1))
            .context("a partition's tap")
    }

    /// The cluster's identity and secret, which every role dials with.
    pub fn role_cluster(&self) -> &RoleCluster {
        &self.cluster
    }

    /// A core's links to the partitions, dialling as compute within `dial_ms` milliseconds: the placement map set, and where
    /// each partition listens, as the core's system link learns them from Members.
    pub fn links_within(&self, dial_ms: u32) -> RoleLinks {
        let listed: Vec<(u32, String)> = self
            .listed
            .iter()
            .map(|(partition, address)| (*partition, address.to_string()))
            .collect();
        links_listing(&self.cluster, self.map.clone(), &listed).with_dial_ms(dial_ms)
    }

    /// A core's links to the partitions, dialling within the protocol's default `transaction_ms`.
    pub fn links(&self) -> RoleLinks {
        self.links_within(DEFAULT_TRANSACTION_MS)
    }

    /// A compute core on this runtime, reading by `times` over `links` (design D2).
    pub fn core_over(&self, links: RoleLinks, times: ReadTimes) -> Rc<ComputeCore> {
        ComputeCore::over(links, self.system.clone(), times)
    }

    /// A compute core on this runtime, reading by `times` over links of its own.
    pub fn core(&self, times: ReadTimes) -> Rc<ComputeCore> {
        self.core_over(self.links(), times)
    }

    /// A client connection's reads on a compute core of this runtime reading by `times`, each counted among `open` until it
    /// ends, as the connection's session runs them.
    pub fn session(&self, times: ReadTimes, open: OpenRequests) -> ReadSession {
        ReadSession::new(CLUSTER, ReadExecutor::new(self.core(times)).with_open(open))
    }

    /// A client connection's reads on `core`, as [`ApartCluster::session`] runs them.
    pub fn session_on(&self, core: Rc<ComputeCore>) -> ReadSession {
        ReadSession::new(CLUSTER, ReadExecutor::new(core))
    }
}

/// Serves `server`'s connections at a port of its own on this runtime, each with a session `session` makes: its address.
async fn serve<S: Session + 'static>(
    cluster: &RoleCluster,
    server: Server,
    session: impl Fn() -> S + 'static,
) -> TestResult<Address> {
    let server = server.with_iterations(ROLE_ITERATIONS);
    let credentials = cluster.secret().credentials()?;
    let listener = Listener::bind(&Address::any_port("127.0.0.1")?).await?;
    let address = listener.local_address()?;
    spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let (server, credentials, connection) =
                (server.clone(), credentials.clone(), session());
            spawn(async move { serve_connection(carrier, &server, credentials, connection).await })
                .detach();
        }
    })
    .detach();
    Ok(address)
}
