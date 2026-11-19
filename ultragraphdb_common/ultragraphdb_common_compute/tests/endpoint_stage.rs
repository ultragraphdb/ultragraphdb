/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Compute's two listeners on one test thread, its owner on its own, the other roles played by doubles and the client over raw
//! frames: what a scenario decides, it decides, and what it asserts, it reads from the frames each side saw.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use compio::runtime::{JoinHandle, spawn};
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_cluster_sequencer::SequencerConfig;
use ultragraphdb_common_compute::{
    AuthThrottle, Authorizer, ClientSession, ComputeCore, DoubleFrame, EndpointCore,
    EndpointLimits, EveryAuthenticated, IriPlace, LoginLimits, ReadTimes, RelayAuthenticator,
    RoleDouble, RoleLinks, SystemLink, TenantGate, iri_place, last_frame, partition_answer,
    scan_answer,
};
use ultragraphdb_common_role::{
    Answer, ConnectionBound, ParkedBytes, ROLE_ITERATIONS, RoleClient, RoleCluster, RoleSecret,
    Server, Session, serve_connection, serve_connection_with,
};
use ultragraphdb_common_service::Readiness;
use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_common_system::{
    SystemHandle, SystemSession, SystemShared, SystemStart, SystemThread, start_system,
};
use ultragraphdb_core_ids::Bucket;
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_transport::{Address, Endpoint};
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_protocol_wire::{
    Abort, Begin, BodyBuilder, BodyView, Command, Commit, ErrorCode, ErrorReply, ErrorReplyView,
    FrameView, Join, MAX_FRAME_SIZE, PatternSlot, QuadPattern, Read, ReadLevel, Register,
    RegisterView, ScanView, TermTable, Write, WriteChange,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::apart::apart_partition::PlayedPartition;

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
    pub check_ms: u64,
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
            check_ms: MetadataConfig::CATALOG_CHECK_MS,
        }
    }
}

/// What each client connection has on its way to a role, in the order they connected.
type Counts = Rc<RefCell<Vec<Option<ParkedBytes>>>>;

/// Compute under a scenario.
pub(crate) struct Stage {
    pub owner: SystemHandle,
    /// The core's link to the group.
    pub system: SystemLink,
    pub cluster: RoleCluster,
    pub role: Address,
    pub client: Address,
    pub links: RoleLinks,
    pub map: PlacementMap,
    counts: Counts,
    directory: PathBuf,
    thread: Option<SystemThread>,
    tasks: Vec<JoinHandle<()>>,
}

fn cluster_of(directory: &std::path::Path) -> TestResult<RoleCluster> {
    let file = directory.join("role-secret");
    std::fs::write(&file, SECRET)?;
    let secret = RoleSecret::read(&file.display().to_string())?;
    Ok(RoleCluster::new(CLUSTER, secret, Duration::from_secs(5)))
}

impl Stage {
    /// Compute of `options`, every request passing `authorizer`, holding the principals `ann` and `bob`.
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
        let start = SystemStart::new(
            MetadataConfig::new()
                .with_sessions_max(options.sessions)?
                .with_catalog_check_ms(options.check_ms)?,
            SequencerConfig::new(1, 5)?,
            options.partitions,
        )
        .with_tenants(options.tenants.clone())
        // The group's quiet time, twice `transaction_ms` from its start, in which every check of kind 1 answers 14 (`PROTOCOL.md`
        // §11.5), has passed before a client's first request: the scenarios here are those of a group past it.
        .with_transaction_ms(1);
        let thread = start_system(
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
        let links = RoleLinks::new(cluster.clone(), "compute".to_string())
            .with_dial_ms(options.limits.transaction_ms());
        let map = owner.map().await.context("a placement map")?;
        let role = Listener::bind(&Address::any_port("127.0.0.1")?).await?;
        let client = Listener::bind(&Address::any_port("127.0.0.1")?).await?;
        let role_address = role.local_address()?;
        let client_address = client.local_address()?;
        let accepting_roles = accept_roles(role, &cluster, &owner)?;
        let (throttle, throttling) = AuthThrottle::new(LoginLimits::default()).serve();
        spawn(throttling).detach();
        // The core dials the system group at the role address the stage serves it at (task 5.2).
        let system = SystemLink::dial(
            role_address.clone(),
            cluster.clone(),
            links.clone(),
            options.limits.transaction_ms(),
            Readiness::default(),
        );
        let deadline = compio::time::interval(WAIT).tick().await + WAIT;
        system
            .answering(deadline)
            .await
            .map_err(|refusal| TestError::new(format!("{refusal:?}")))?;
        if options.older_map {
            links.set_map(PlacementMap::even(1, NodeId::new(1))?);
        } else {
            links.set_map(map.clone());
        }
        // The core's reads run in it, over its links to the partitions, beside its Bind streams (design D2).
        let compute = ComputeCore::over(
            links.clone(),
            role_address.clone(),
            ReadTimes::new(options.limits.scan_stall(), Duration::from_secs(600)),
        );
        let core = EndpointCore::new(
            system.clone(),
            throttle,
            links.clone(),
            TenantGate::new(authorizer),
            options.limits,
        )
        .with_compute(compute);
        let counts = Counts::default();
        let accepting_clients =
            accept_clients(client, unknown, options.connections, core, counts.clone())?;
        Ok(Self {
            owner,
            system,
            cluster,
            role: role_address,
            client: client_address,
            links,
            map,
            counts,
            directory: held,
            thread: Some(thread),
            tasks: vec![accepting_roles, accepting_clients],
        })
    }

    /// Compute of `options`, the authorizer of this stage.
    pub(crate) async fn start(options: Options) -> TestResult<Self> {
        Self::start_gated(options, EveryAuthenticated).await
    }

    /// Stops compute's listeners and its owner, for a scenario restarting it over the same directory.
    pub(crate) async fn stop(mut self) -> TestResult {
        self.tasks.clear();
        self.system.stop();
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

    /// The bytes the client connection that connected `at`th has on its way to a role.
    pub(crate) fn on_way(&self, at: usize) -> usize {
        self.counts
            .borrow()
            .get(at)
            .and_then(Option::as_ref)
            .map_or(0, ParkedBytes::count)
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

    /// Members read over the stage's link, as compute reads it at its start until every partition is listed (design D9): the
    /// doubles join after the stage starts, so each join is followed by this read.
    pub(crate) async fn read_members(&self) -> TestResult {
        self.system
            .members()
            .await
            .map_err(|refusal| TestError::new(format!("{refusal:?}")))
    }

    /// A partition played by `script`, joined as `partition`, and Members read.
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
        self.read_members().await?;
        Ok(double)
    }

    /// A partition answering as nothing goes wrong.
    pub(crate) async fn partition(&self, partition: u32) -> TestResult<RoleDouble> {
        self.partition_with(partition, |frame| {
            partition_answer(frame, CLUSTER, BIND_VERSION)
        })
        .await
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

/// A partition answering as nothing goes wrong but each read's Scan, which it takes and answers none until a step releases it.
pub(crate) fn holding_reads(frame: &DoubleFrame) -> Answer {
    if frame.command() == Command::Scan {
        return Answer::Open;
    }
    partition_answer(frame, CLUSTER, BIND_VERSION)
}

/// A partition answering each read's Scan with `refusal`, and everything else as nothing goes wrong.
pub(crate) fn refusing_reads(refusal: ErrorReply) -> impl Fn(&DoubleFrame) -> Answer + 'static {
    move |frame| {
        if frame.command() == Command::Scan {
            return Answer::Refuse(refusal.clone());
        }
        partition_answer(frame, CLUSTER, BIND_VERSION)
    }
}

/// A partition answering each read's Scan from `played`'s table while `holding` is unset, taking it and answering none while it
/// is set, and everything else as nothing goes wrong.
pub(crate) fn rows_held(
    played: &Rc<PlayedPartition>,
    holding: &Rc<Cell<bool>>,
) -> impl Fn(&DoubleFrame) -> Answer + 'static {
    let (played, holding) = (Rc::clone(played), Rc::clone(holding));
    move |frame| {
        if frame.command() != Command::Scan {
            return partition_answer(frame, CLUSTER, BIND_VERSION);
        }
        if holding.get() {
            return Answer::Open;
        }
        let Some(mut frames) = scan_frames(frame, &played).map(Vec::into_iter) else {
            return Answer::Done;
        };
        let Some(first) = frames.next() else {
            return Answer::Done;
        };
        frames.for_each(|more| frame.answer_with(more));
        Answer::Reply(first)
    }
}

/// The frames `played` answers the Scan `frame` with from its table; `None` for a Scan it cannot read.
fn scan_frames(frame: &DoubleFrame, played: &Rc<PlayedPartition>) -> Option<Vec<Vec<u8>>> {
    let view = view_frame(frame.bytes()).ok()?;
    let scan = ScanView::from_frame(&view).ok()?;
    played.frames_for(frame.stream(), &scan).ok()
}

/// Answers the read Scan at `index` of `partition` from `played`'s table.
pub(crate) fn release_rows(
    partition: &RoleDouble,
    index: usize,
    played: &Rc<PlayedPartition>,
) -> TestResult {
    let scans = read_scans(partition);
    let scan = scans.get(index).context("the read's Scan")?;
    let frames = scan_frames(scan, played).context("the Scan's answer")?;
    frames.into_iter().for_each(|frame| scan.answer_with(frame));
    Ok(())
}

/// Whether the Scan `frame` holds a read's version alone, naming one key in every range of several, as a pin does.
fn is_pin(frame: &DoubleFrame) -> bool {
    let Ok(view) = view_frame(frame.bytes()) else {
        return false;
    };
    let Ok(scan) = ScanView::from_frame(&view) else {
        return false;
    };
    let firsts: Vec<Vec<u8>> = scan.ranges().map(|range| range.first().to_vec()).collect();
    firsts.len() > 1 && firsts.iter().all(|first| Some(first) == firsts.first())
}

/// The Scans `partition` took for reads, in order, a pin's left out.
pub(crate) fn read_scans(partition: &RoleDouble) -> Vec<DoubleFrame> {
    partition
        .frames(Command::Scan)
        .into_iter()
        .filter(|frame| !is_pin(frame))
        .collect()
}

/// Answers the read Scan at `index` of `partition` as a partition holding nothing at `version` does: the read ends with a batch
/// of no row.
pub(crate) fn release(partition: &RoleDouble, index: usize, version: u64) -> TestResult {
    let scans = read_scans(partition);
    let scan = scans.get(index).context("the read's Scan")?;
    match scan_answer(scan, CLUSTER, version) {
        Answer::Reply(frame) => {
            scan.answer_with(frame);
            Ok(())
        }
        Answer::Open | Answer::Done | Answer::Refuse(_) | Answer::Close(_) => {
            Err(TestError::new("the Scan has no answer"))
        }
    }
}

/// Refuses the read Scan at `index` of `partition` with `refusal`.
pub(crate) fn refuse_scan(
    partition: &RoleDouble,
    index: usize,
    refusal: &ErrorReply,
) -> TestResult {
    let scans = read_scans(partition);
    let scan = scans.get(index).context("the read's Scan")?;
    scan.answer_with(last_frame(CLUSTER, scan.stream(), refusal)?);
    Ok(())
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

/// The frame `bytes` a double took, checked whole against the cluster and the largest frame.
pub(crate) fn view_frame(bytes: &[u8]) -> TestResult<FrameView<'_>> {
    Ok(FrameView::validate(bytes, CLUSTER, MAX_FRAME_SIZE)?)
}

/// Accepts the role connections of `listener`, each served by a session of its own.
fn accept_roles(
    listener: Listener,
    cluster: &RoleCluster,
    owner: &SystemHandle,
) -> TestResult<JoinHandle<()>> {
    let roles = Server::new(CLUSTER, "compute", Endpoint::System)?.with_iterations(ROLE_ITERATIONS);
    let credentials = cluster.secret().credentials()?;
    let shared = SystemShared::new(cluster.clone(), owner.clone());
    Ok(spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let (roles, credentials) = (roles.clone(), credentials.clone());
            let session = SystemSession::new(shared.clone());
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
    counts: Counts,
) -> TestResult<JoinHandle<()>>
where
    A: Authorizer + Clone + 'static,
{
    let clients = Server::new(CLUSTER, "compute", Endpoint::Client)?
        .with_unknown_secret(unknown)
        .with_bound(ConnectionBound::new(connections));
    // The Welcome states the credit a read's stream starts with, which the connection meters (`PROTOCOL.md` §11.3): the
    // protocol's default, or the one frame a scenario of spent credit takes.
    let clients = if core.limits().initial_credit() == 1 {
        clients.with_welcome(|welcome| welcome.with_initial_credit(1))
    } else {
        clients
    };
    Ok(spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let server = clients.clone();
            let relay = RelayAuthenticator::default();
            let session = ClientSession::new(core.clone(), relay.clone(), carrier.peer_ip());
            counts.borrow_mut().push(session.parked());
            spawn(async move {
                serve_connection_with(carrier, &server, relay, session).await;
            })
            .detach();
        }
    }))
}
