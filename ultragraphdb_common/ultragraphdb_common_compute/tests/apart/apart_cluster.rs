/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A played frontend and played partitions on the test's runtime, and compute on a runtime of its own on another thread.

use std::future::Future;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use compio::runtime::{Runtime, spawn, spawn_blocking};
use futures::channel::oneshot;
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_compute::{ComputeCore, ComputeSession, ReadTimes};
use ultragraphdb_common_role::{
    Answer, COMPUTE_WELCOME_CREDIT, OpenRequests, PARTITION_INITIAL_CREDIT, ROLE_ITERATIONS,
    RoleClient, RoleCluster, RoleSecret, Server, Session, serve_connection,
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

/// The played cluster's identity.
pub const CLUSTER: u64 = 7;

/// How long one test's work may take in all.
const TEST_WAIT: Duration = Duration::from_secs(30);

/// How long the test waits for compute to listen.
const LISTEN_WAIT: Duration = Duration::from_secs(5);

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
        let server = Server::new(CLUSTER, "frontend", Endpoint::Frontend)?;
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

    /// Compute on a thread of its own, reading by `times` and counting its open reads in `open`: a connection to it, as the
    /// frontend dials one. Its runtime serves that one connection, so a read that never yields holds that thread alone.
    pub async fn compute(&self, times: ReadTimes, open: OpenRequests) -> TestResult<RoleClient> {
        let (told, listening) = oneshot::channel::<TestResult<String>>();
        let (cluster, frontend) = (self.cluster.clone(), self.frontend.clone());
        spawn_blocking(move || {
            let runtime = match Runtime::new() {
                Ok(runtime) => runtime,
                Err(error) => return drop(told.send(Err(error.into()))),
            };
            runtime.block_on(serve_compute((cluster, frontend), times, open, told));
        })
        .detach();
        let address = compio::time::timeout(LISTEN_WAIT, listening)
            .await
            .map_err(|_| TestError::new("compute did not listen"))???;
        let password = self.cluster.secret().password();
        let address = Address::parse(&address)?;
        let dial = RoleClient::dial(&address, CLUSTER, "frontend", password);
        // Compute serves on a runtime of its own, so a compute that stops answering fails the dial here rather than holding it.
        Ok(compio::time::timeout(LISTEN_WAIT, dial)
            .await
            .map_err(|_| TestError::new("compute did not answer the dial"))??)
    }
}

/// Serves compute's one connection on this runtime, telling `told` where it listens or why it cannot.
async fn serve_compute(
    (cluster, frontend): (RoleCluster, Address),
    times: ReadTimes,
    open: OpenRequests,
    told: oneshot::Sender<TestResult<String>>,
) {
    let listening = async {
        let listener = Listener::bind(&Address::any_port("127.0.0.1")?).await?;
        let server = Server::new(CLUSTER, "role", Endpoint::Compute)?
            .with_welcome(|welcome| welcome.with_initial_credit(COMPUTE_WELCOME_CREDIT))
            .with_iterations(ROLE_ITERATIONS);
        let credentials = cluster.secret().credentials()?;
        TestResult::Ok((listener, server, credentials))
    };
    let (listener, server, credentials) = match listening.await {
        Ok(listening) => listening,
        Err(error) => return drop(told.send(Err(error))),
    };
    drop(
        told.send(
            listener
                .local_address()
                .map(|address| address.to_string())
                .map_err(Into::into),
        ),
    );
    let core = ComputeCore::new(cluster, frontend, times, "compute");
    if let Ok(carrier) = listener.accept().await {
        let session = ComputeSession::new(core).with_open(open);
        serve_connection(carrier, &server, credentials, session).await;
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
