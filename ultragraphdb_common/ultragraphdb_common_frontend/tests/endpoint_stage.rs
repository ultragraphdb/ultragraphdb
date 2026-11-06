/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A frontend's two listeners on one test thread, its owner on its own, the other roles played by doubles and the client over raw
//! frames: what a scenario decides, it decides, and what it asserts, it reads from the frames each side saw.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use compio::runtime::{JoinHandle, spawn};
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_cluster_sequencer::SequencerConfig;
use ultragraphdb_common_frontend::{
    AskedCredentials, Authorizer, ClientSession, DoubleFrame, EndpointCore, EndpointLimits,
    EveryAuthenticated, IriPlace, OwnerHandle, OwnerStart, OwnerThread, RoleDouble, RoleLinks,
    RoleSession, RoleShared, TenantGate, batch_frame, empty_batch, iri_place, partition_answer,
    start_owner,
};
use ultragraphdb_common_role::{
    Answer, ConnectionBound, ROLE_ITERATIONS, RoleClient, RoleCluster, RoleSecret, Server,
    serve_connection,
};
use ultragraphdb_common_service::Readiness;
use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_core_ids::Bucket;
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_transport::{Address, Endpoint};
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_protocol_wire::{
    Abort, Begin, BodyBuilder, BodyView, Command, Commit, ErrorCode, ErrorReplyView, FrameView,
    Join, MAX_FRAME_SIZE, PatternSlot, QuadPattern, Read, ReadLevel, Register, RegisterView,
    TermTable, Write, WriteChange,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

pub(crate) const CLUSTER: u64 = 7;
pub(crate) const PREDICATE: &str = "http://ex.org/p";
pub(crate) const OBJECT: &str = "http://ex.org/o";
pub(crate) const BIND_VERSION: u64 = 5 << 22;

const SECRET: &str = "the role secret of a stage";
const WAIT: Duration = Duration::from_secs(5);

static DIRECTORIES: AtomicU32 = AtomicU32::new(0);

/// A directory no other scenario uses.
pub(crate) fn fresh() -> TestResult<PathBuf> {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let directory = root.join(format!(
        "stage-{}-{}",
        std::process::id(),
        DIRECTORIES.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&directory)?;
    Ok(directory)
}

/// What a stage is built of.
#[derive(Debug, Clone)]
pub(crate) struct Options {
    pub partitions: u32,
    pub limits: EndpointLimits,
    pub connections: u64,
    pub tenants: Vec<u64>,
    pub sessions: u32,
    pub directory: Option<PathBuf>,
    pub older_map: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            partitions: 1,
            limits: EndpointLimits::new(1 << 22, Duration::from_secs(30)),
            connections: 64,
            tenants: Vec::new(),
            sessions: 8,
            directory: None,
            older_map: false,
        }
    }
}

/// The frontend under a scenario.
pub(crate) struct Stage {
    pub owner: OwnerHandle,
    pub cluster: RoleCluster,
    pub role: Address,
    pub client: Address,
    pub links: RoleLinks,
    pub map: PlacementMap,
    directory: PathBuf,
    thread: Option<OwnerThread>,
    tasks: Vec<JoinHandle<()>>,
}

fn cluster_of(directory: &std::path::Path) -> TestResult<RoleCluster> {
    let file = directory.join("role-secret");
    std::fs::write(&file, SECRET)?;
    let secret = RoleSecret::read(&file.display().to_string())?;
    Ok(RoleCluster::new(CLUSTER, secret, Duration::from_secs(5)))
}

impl Stage {
    /// A frontend of `options`, every request passing `authorizer`, holding the principals `ann` and `bob`.
    pub(crate) async fn start_gated<A>(options: Options, authorizer: A) -> TestResult<Self>
    where
        A: Authorizer + Clone + 'static,
    {
        let directory = match &options.directory {
            Some(directory) => directory.clone(),
            None => fresh()?,
        };
        let held = directory.clone();
        let cluster = cluster_of(&directory)?;
        let verifier = ScramVerifier::derive("pw", b"salt", 4_096)?;
        let start = OwnerStart::new(
            MetadataConfig::new().with_sessions_max(options.sessions)?,
            SequencerConfig::new(1, 5)?,
            options.partitions,
        )
        .with_tenants(options.tenants.clone());
        let thread = start_owner(
            ServiceConfig::default(),
            CLUSTER,
            directory,
            start,
            vec![
                ("ann".to_string(), verifier.clone()),
                ("bob".to_string(), verifier),
            ],
        )
        .await?;
        let unknown = thread.unknown_secret();
        let owner = thread.handle().clone();
        let links = RoleLinks::new(cluster.clone(), "frontend".to_string());
        let map = owner.map().await.context("a placement map")?;
        if options.older_map {
            links.set_map(PlacementMap::even(1, NodeId::new(1))?);
        } else {
            links.set_map(map.clone());
        }
        let role = Listener::bind(&Address::any_port("127.0.0.1")?).await?;
        let client = Listener::bind(&Address::any_port("127.0.0.1")?).await?;
        let role_address = role.local_address()?;
        let client_address = client.local_address()?;
        let accepting_roles = accept_roles(role, &cluster, &owner, &links)?;
        let core = EndpointCore::new(
            owner.clone(),
            links.clone(),
            TenantGate::new(authorizer),
            options.limits,
        );
        let accepting_clients = accept_clients(client, unknown, options.connections, core)?;
        Ok(Self {
            owner,
            cluster,
            role: role_address,
            client: client_address,
            links,
            map,
            directory: held,
            thread: Some(thread),
            tasks: vec![accepting_roles, accepting_clients],
        })
    }

    /// A frontend of `options`, the authorizer of this stage.
    pub(crate) async fn start(options: Options) -> TestResult<Self> {
        Self::start_gated(options, EveryAuthenticated).await
    }

    /// Stops the frontend's listeners and its owner, for a scenario restarting it over the same directory.
    pub(crate) async fn stop(mut self) -> TestResult {
        self.tasks.clear();
        let thread = self.thread.take().context("an owner")?;
        drop(self.owner);
        drop(self.links);
        thread.stopped().await?;
        Ok(())
    }

    /// The directory it keeps its logs in.
    pub(crate) fn directory(&self) -> Option<PathBuf> {
        Some(self.directory.clone())
    }

    /// `ann`'s connection.
    pub(crate) async fn ann(&self) -> TestResult<Client> {
        self.as_principal("ann").await
    }

    /// `name`'s connection, its password `pw`.
    pub(crate) async fn as_principal(&self, name: &str) -> TestResult<Client> {
        Ok(Client::over(
            RoleClient::dial(&self.client, CLUSTER, name, "pw").await?,
        ))
    }

    /// A partition played by `script`, joined as `partition`.
    pub(crate) async fn partition_with<F>(
        &self,
        partition: u32,
        script: F,
    ) -> TestResult<RoleDouble>
    where
        F: Fn(&DoubleFrame) -> Answer + 'static,
    {
        let double = RoleDouble::start(&self.cluster, Endpoint::Partition, script).await?;
        let join = Join::storage(double.address(), vec![partition]);
        double
            .join(&self.role, &self.cluster, "storage", &join)
            .await
            .map_err(TestError::new)?;
        Ok(double)
    }

    /// A partition answering as nothing goes wrong.
    pub(crate) async fn partition(&self, partition: u32) -> TestResult<RoleDouble> {
        self.partition_with(partition, |frame| {
            partition_answer(frame, CLUSTER, BIND_VERSION)
        })
        .await
    }

    /// A compute process played by `script`, joined.
    pub(crate) async fn compute_with<F>(&self, script: F) -> TestResult<RoleDouble>
    where
        F: Fn(&DoubleFrame) -> Answer + 'static,
    {
        let double = RoleDouble::start(&self.cluster, Endpoint::Compute, script).await?;
        let join = Join::compute(double.address());
        double
            .join(&self.role, &self.cluster, "compute", &join)
            .await
            .map_err(TestError::new)?;
        Ok(double)
    }

    /// A compute process answering each read with one batch of no row, of partition 1.
    pub(crate) async fn compute(&self) -> TestResult<RoleDouble> {
        self.compute_with(answer_empty).await
    }

    /// An IRI of the tenant 0 that the stage's map places in `partition`.
    pub(crate) fn on(&self, partition: u32) -> TestResult<String> {
        self.on_tagged(partition, "")
    }

    /// As [`Stage::on`], a different IRI for each `tag`.
    pub(crate) fn on_tagged(&self, partition: u32, tag: &str) -> TestResult<String> {
        (0..1 << 16)
            .map(|at| format!("http://ex.org/{tag}s{at}"))
            .find(|iri| match iri_place(0, iri) {
                IriPlace::Ordinary(bucket) => self.owner_of(bucket) == partition,
                IriPlace::Skolem(_) | IriPlace::Nobody | IriPlace::TooLong => false,
            })
            .context("an IRI of the partition")
    }

    fn owner_of(&self, bucket: Bucket) -> u32 {
        self.map
            .partition_of(bucket, self.map.latest_change())
            .get()
    }
}

/// One batch of no row, of partition 1, for each read.
pub(crate) fn answer_empty(frame: &DoubleFrame) -> Answer {
    empty_answer(frame, 1)
}

/// One batch of no row, of `partition`, for each read.
pub(crate) fn empty_answer(frame: &DoubleFrame, partition: u32) -> Answer {
    batch_frame(CLUSTER, frame.stream(), &empty_batch(4, partition), true)
        .map_or(Answer::Done, Answer::Reply)
}

/// What a client read, in a form a step compares.
#[derive(Debug)]
pub(crate) struct Got {
    pub command: Command,
    pub code: Option<ErrorCode>,
    pub detail: u64,
    pub message: String,
    pub bytes: Vec<u8>,
}

/// A client over raw frames.
pub(crate) struct Client {
    inner: RoleClient,
    early: VecDeque<(u64, Got)>,
}

impl Client {
    pub(crate) fn over(inner: RoleClient) -> Self {
        Self {
            inner,
            early: VecDeque::new(),
        }
    }

    /// Sends `body` on a new stream: the stream's number.
    pub(crate) async fn send<B: BodyBuilder>(&mut self, body: &B) -> TestResult<u64> {
        self.inner.send_only(body).await?;
        Ok(self.inner.last_stream())
    }

    /// Sends `body` on `stream`.
    pub(crate) async fn on<B: BodyBuilder>(&mut self, stream: u64, body: &B) -> TestResult {
        Ok(self.inner.send_on(stream, body).await?)
    }

    /// The next frame the connection reads, on any stream.
    async fn next(&mut self) -> TestResult<(u64, Got)> {
        let frame = compio::time::timeout(WAIT, self.inner.next_frame())
            .await
            .map_err(|_| TestError::new("no frame came"))??;
        let view = frame.view()?;
        let stream = view.header().stream();
        let command = view.command();
        let (code, detail, message) = if command == Command::Error {
            let error = ErrorReplyView::from_frame(&view)?;
            (
                Some(error.code()),
                error.detail(),
                error.message().to_string(),
            )
        } else {
            (None, 0, String::new())
        };
        let bytes = frame.bytes().to_vec();
        Ok((
            stream,
            Got {
                command,
                code,
                detail,
                message,
                bytes,
            },
        ))
    }

    /// The next frame on `stream`, keeping what comes on others.
    pub(crate) async fn got(&mut self, stream: u64) -> TestResult<Got> {
        if let Some(at) = self.early.iter().position(|(on, _)| *on == stream) {
            return self.early.remove(at).map(|(_, got)| got).context("a frame");
        }
        loop {
            let (on, got) = self.next().await?;
            if on == stream {
                return Ok(got);
            }
            self.early.push_back((on, got));
        }
    }

    /// The frames already read on `stream` and not yet taken, in their order.
    pub(crate) fn taken(&mut self, stream: u64) -> Vec<Got> {
        let (taken, kept): (VecDeque<_>, VecDeque<_>) = std::mem::take(&mut self.early)
            .into_iter()
            .partition(|(on, _)| *on == stream);
        self.early = kept;
        taken.into_iter().map(|(_, got)| got).collect()
    }

    /// The Error next on `stream`: its code and detail.
    pub(crate) async fn refused(&mut self, stream: u64) -> TestResult<(ErrorCode, u64)> {
        let got = self.got(stream).await?;
        Ok((got.code.context("an Error")?, got.detail))
    }

    /// That nothing comes on `stream` within `ms`: a frame on it fails, and so does a connection that fails meanwhile, since what
    /// it would have carried is unknown.
    pub(crate) async fn silent(&mut self, stream: u64, ms: u64) -> TestResult {
        let waited = compio::time::timeout(Duration::from_millis(ms), self.got(stream)).await;
        match waited {
            Err(_) => Ok(()),
            Ok(Err(error)) => Err(TestError::new(format!(
                "the connection failed while stream {stream} was to stay silent: {error}"
            ))),
            Ok(Ok(got)) => Err(TestError::new(format!(
                "{:?} came on stream {stream}",
                got.command
            ))),
        }
    }

    /// A read-write transaction of tenant `tenant`: its stream, the Begin answered.
    pub(crate) async fn begin(&mut self, tenant: u64) -> TestResult<u64> {
        let stream = self.send(&Begin::new(tenant, 0, false)).await?;
        assert_eq!(self.got(stream).await?.command, Command::Begin);
        Ok(stream)
    }

    /// A read-only transaction at `level`: its stream, the Begin answered.
    pub(crate) async fn begin_read_only(&mut self, level: ReadLevel) -> TestResult<u64> {
        let stream = self.send(&Begin::new(0, 0, true).with_level(level)).await?;
        assert_eq!(self.got(stream).await?.command, Command::Begin);
        Ok(stream)
    }

    /// A session registered for the connection's principal.
    pub(crate) async fn register(&mut self) -> TestResult<[u8; 16]> {
        let stream = self.send(&Register::new([0; 16])).await?;
        let got = self.got(stream).await?;
        let view = view_frame(&got.bytes)?;
        Ok(RegisterView::from_frame(&view)?.session())
    }

    /// A Write of `subject` on `transaction`'s stream.
    pub(crate) async fn write(&mut self, transaction: u64, subject: &str) -> TestResult {
        self.on(transaction, &write_of(subject)).await
    }

    /// A read of `subject` in `transaction`: its stream.
    pub(crate) async fn read(&mut self, transaction: u64, subject: &str) -> TestResult<u64> {
        self.send(&read_of(transaction, subject)).await
    }

    /// A Commit through no session on `transaction`'s stream.
    pub(crate) async fn commit(&mut self, transaction: u64) -> TestResult {
        self.on(transaction, &Commit::new([0; 16], 0, 0)).await
    }

    /// An Abort on `transaction`'s stream.
    pub(crate) async fn abort(&mut self, transaction: u64) -> TestResult {
        self.on(transaction, &Abort).await
    }
}

/// A Write inserting `subject`'s statement.
pub(crate) fn write_of(subject: &str) -> Write {
    let mut terms = TermTable::new();
    let change = insert_into(&mut terms, subject);
    Write::new(terms, vec![change])
}

/// An insert of `subject`'s statement.
pub(crate) fn insert_into(terms: &mut TermTable, subject: &str) -> WriteChange {
    WriteChange::Insert {
        graph: None,
        subject: terms.iri(subject),
        predicate: terms.iri(PREDICATE),
        object: terms.iri(OBJECT),
    }
}

/// A read of `subject`'s statements in `transaction`.
pub(crate) fn read_of(transaction: u64, subject: &str) -> Read {
    let mut terms = TermTable::new();
    let pattern = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Bound(terms.iri(subject)),
        PatternSlot::Unbound,
        PatternSlot::Unbound,
    );
    Read::new(transaction, 0, terms, pattern, Vec::new())
}

/// Waits until `holds`, each millisecond, or says what it waited for.
pub(crate) async fn until(what: &str, holds: impl Fn() -> bool) -> TestResult {
    for _ in 0..5_000 {
        if holds() {
            return Ok(());
        }
        compio::time::sleep(Duration::from_millis(1)).await;
    }
    Err(TestError::new(format!("{what} did not come to hold")))
}

/// Lets `ms` pass.
pub(crate) async fn pause(ms: u64) {
    compio::time::sleep(Duration::from_millis(ms)).await;
}

/// The `transaction` identity's read version in a ComputeRead `frame` the double took.
pub(crate) fn view_frame(bytes: &[u8]) -> TestResult<FrameView<'_>> {
    Ok(FrameView::validate(bytes, CLUSTER, MAX_FRAME_SIZE)?)
}

/// Accepts the role connections of `listener`, each served by a session of its own.
fn accept_roles(
    listener: Listener,
    cluster: &RoleCluster,
    owner: &OwnerHandle,
    links: &RoleLinks,
) -> TestResult<JoinHandle<()>> {
    let roles =
        Server::new(CLUSTER, "frontend", Endpoint::Frontend)?.with_iterations(ROLE_ITERATIONS);
    let credentials = cluster.secret().credentials()?;
    let shared = RoleShared::new(
        cluster.clone(),
        owner.clone(),
        links.clone(),
        Readiness::default(),
    );
    Ok(spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let (roles, credentials) = (roles.clone(), credentials.clone());
            let session = RoleSession::new(shared.clone());
            spawn(async move {
                serve_connection(carrier, &roles, credentials, session).await;
            })
            .detach();
        }
    }))
}

/// Accepts the client connections of `listener` into `core`, as many held at once as `connections` allows.
fn accept_clients<A>(
    listener: Listener,
    unknown: [u8; 32],
    connections: u64,
    core: EndpointCore<A>,
) -> TestResult<JoinHandle<()>>
where
    A: Authorizer + Clone + 'static,
{
    let clients = Server::new(CLUSTER, "frontend", Endpoint::Client)?
        .with_unknown_secret(unknown)
        .with_bound(ConnectionBound::new(connections));
    Ok(spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let server = clients.clone();
            let credentials = AskedCredentials::default();
            let session = ClientSession::new(core.clone(), credentials.clone(), carrier.peer_ip());
            spawn(async move {
                serve_connection(carrier, &server, credentials, session).await;
            })
            .detach();
        }
    }))
}
