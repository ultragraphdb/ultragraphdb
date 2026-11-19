/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A played frontend and played partitions on the test's runtime, and compute's core beside them.

use std::future::Future;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_compute::{ComputeCore, ReadExecutor, ReadTimes};
use ultragraphdb_common_role::{
    Answer, OpenRequests, PARTITION_INITIAL_CREDIT, ROLE_ITERATIONS, RoleCluster, RoleSecret,
    Server, Session, serve_connection,
};
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_protocol_transport::{Address, Endpoint, ReceivedFrame};
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BucketRange, Command, ErrorCode, ErrorReply, FLAG_LAST, Header, Members,
    Placement, PreferredLeader,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use super::apart_partition::PlayedPartition;
use super::apart_session::ReadSession;

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

/// One connection at the played frontend: the placement and where each partition listens.
#[derive(Debug)]
struct FrontendConnection {
    placement: Rc<Placement>,
    members: Rc<Members>,
}

impl Session for FrontendConnection {
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

/// The even placement over `partitions`, as a frontend's first start forms it.
pub fn even_map(partitions: u32) -> TestResult<PlacementMap> {
    Ok(PlacementMap::even(partitions, NodeId::new(1))?)
}

/// How long a test waits for a core's links to let the streams of ended reads go, a dropped task's included, and how often it
/// looks.
const STREAMS_WAIT: Duration = Duration::from_secs(2);
const STREAMS_LOOK: Duration = Duration::from_millis(10);

/// The links `core` keeps and how many of them still hold a stream, as its `Debug` lists each link's table of streams, once the
/// streams of reads that ended have gone or past [`STREAMS_WAIT`].
pub async fn links_holding_streams(core: &ComputeCore) -> (usize, usize) {
    let held = || {
        let listed = format!("{core:?}");
        let links = listed.matches("streams: ").count();
        let empty = listed.matches("streams: RefCell { value: {} }").count();
        (links, links.saturating_sub(empty))
    };
    let gone = async {
        while held().1 > 0 {
            compio::time::sleep(STREAMS_LOOK).await;
        }
    };
    // Past the wait, what is still held is the answer.
    let _waited = compio::time::timeout(STREAMS_WAIT, gone).await;
    held()
}

/// A cluster played on the test's runtime.
#[derive(Debug)]
pub struct ApartCluster {
    cluster: RoleCluster,
    frontend: Address,
    partitions: Vec<Rc<PlayedPartition>>,
}

impl ApartCluster {
    /// A frontend and `partitions` partitions, placed evenly, served on this runtime.
    pub async fn start(partitions: u32) -> TestResult<Self> {
        let cluster = RoleCluster::new(
            CLUSTER,
            RoleSecret::read(&secret_file()?)?,
            Duration::from_secs(5),
        );
        let mut members = Members::new();
        let mut played = Vec::new();
        for partition in 1..=partitions {
            let held = PlayedPartition::new(partition);
            let server = Server::new(CLUSTER, "storage", Endpoint::Partition)?
                .with_welcome(|welcome| welcome.with_initial_credit(PARTITION_INITIAL_CREDIT));
            let serving = Rc::clone(&held);
            let address = serve(&cluster, server, move || serving.connection()).await?;
            members.push_storage(partition, address.to_string());
            played.push(held);
        }
        let mut placement = Placement::reply(0, 0, 1, 0);
        for (range, partition) in even_map(partitions)?.ranges_at(Epoch::ZERO) {
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
        let frontend = serve(&cluster, server, move || FrontendConnection {
            placement: Rc::clone(&placement),
            members: Rc::clone(&members),
        })
        .await?;
        Ok(Self {
            cluster,
            frontend,
            partitions: played,
        })
    }

    /// Played partition `partition`.
    pub fn partition(&self, partition: u32) -> TestResult<&Rc<PlayedPartition>> {
        self.partitions
            .get((partition as usize).wrapping_sub(1))
            .context("a played partition")
    }

    /// A compute core on this runtime, reading by `times`.
    pub fn core(&self, times: ReadTimes) -> Rc<ComputeCore> {
        ComputeCore::new(
            self.cluster.clone(),
            self.frontend.clone(),
            times,
            "compute",
        )
    }

    /// A client connection's reads on a compute core of this runtime reading by `times`, each counted among `open` until it
    /// ends, as the connection's session runs them.
    pub fn session(&self, times: ReadTimes, open: OpenRequests) -> ReadSession {
        ReadSession::new(CLUSTER, ReadExecutor::new(self.core(times)).with_open(open))
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
