/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A partition's core in the test's process, as the storage role runs one: partition 1 of a placement map over fjall and its
//! write-ahead log in a directory of its own, listening on a loopback address where each connection is a [`StorageSession`],
//! and served by [`PartitionCore::run`] beside the test until the test ends. A test dials it as compute does and reads its
//! frames; the channels the role's shell holds (the closes it delivers, what the core asks of the frontend, the discard
//! versions it raises, the inbox itself) are the test's.

use std::convert::Infallible;
use std::sync::mpsc::{Receiver, RecvError, RecvTimeoutError, Sender};
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use ultragraphdb_cluster_metadata::LeasedBlocks;
use ultragraphdb_cluster_partition::{
    Partition, PartitionHost, PartitionMailbox, PartitionMessage, RemoteMessage,
};
use ultragraphdb_cluster_placement::{NodeId, PartitionId, PlacementMap};
use ultragraphdb_common_role::{
    PARTITION_INITIAL_CREDIT, ROLE_ITERATIONS, RoleClient, RoleCluster, RoleSecret, Server, listen,
    serve_connection,
};
use ultragraphdb_common_service::Stopping;
use ultragraphdb_common_storage::{
    CoreChannels, CoreMessage, CoreParts, PartitionCore, Peers, RefillBlocks, Retention,
    StorageHost, StoragePartition, StorageSession,
};
use ultragraphdb_core_ids::{Epoch, SystemId, TypePrefix};
use ultragraphdb_core_keys::{Graph, QuadKey};
use ultragraphdb_core_mapping::RandomSource;
use ultragraphdb_protocol_transport::{Address, Endpoint, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    Bind, BindKind, BindView, BodyView, Command, Commit, CommitView, Credit, EntriesSequence,
    ErrorCode, ErrorReplyView, FLAG_LAST, ObjectRecords, Scan, ScanAt, ScanRange, TermTable, Write,
    WriteChange,
};
use ultragraphdb_store_apply::ChangesetApplier;
use ultragraphdb_store_kv::{Durability, FjallStore, MokaListCache, ThreadExecutor};
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_quads::{QuadStore, term_key};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_tool_test::TestDir;
use ultragraphdb_traits_blocks::SequenceBlock;
use ultragraphdb_traits_store::Executor;
use ultragraphdb_transactions_commit::{CloseRequest, Pipeline, RoundBounds};
use ultragraphdb_transactions_log::{CompioStorage, Wal, WalConfig};

use crate::types::partition_core::yielding_session::Yielding;

/// The cluster every frame names.
pub(crate) const CLUSTER: u64 = 7;
/// The partition the core hosts.
pub(crate) const PARTITION: u32 = 1;
/// The tenant the placement map lists a run for.
pub(crate) const TENANT: u64 = 0;
/// The principal the tests read as.
pub(crate) const PRINCIPAL: u64 = 1;
/// The role secret every connection authenticates with.
const PASSWORD: &str = "s3cret";
/// How long a test waits for any one frame.
pub(crate) const DUE: Duration = Duration::from_secs(10);
/// How long [`on_core_in_time`] waits for its test to end once its core is set up: a core that never yields holds the test's
/// thread, [`DUE`]'s timers with it, so only a thread apart can end the test. The clock starts when the test begins and stops
/// when it ends, before the core stops, so neither the setup nor the teardown, which open and close a store and a log and slow
/// with the machine's load, counts against it. The test dials, authenticates and scans in about 20 ms, and in at most 218 ms with
/// twelve suites beside 96 busy loops at a load average of up to 158 (measured 2026-11-08), so 30 s leaves a margin of over a
/// hundred times that. A hung test still fails within setup and 30 s, the mutated suite in under a minute, well inside the 160 s
/// timeout `cargo mutants` derived for this crate in its runs 2 and 3.
const TEST_DUE: Duration = Duration::from_secs(30);
/// The posting lists' cache.
const CACHE_BYTES: u64 = 1 << 20;
/// The bytes each connection may hold parked.
const HELD_BYTES: usize = 1 << 20;
/// The sequences each leased block holds, one block of each type: more than a test allocates.
const BLOCK: u64 = 1 << 16;

/// How the test's core is set up.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Setup {
    /// The partitions of the even placement map; the core hosts the first, and the others are in no process of the test.
    pub(crate) partitions: u32,
    /// How long it retains a closed epoch, in milliseconds.
    pub(crate) retain_ms: u64,
    /// How long it keeps a Scan with no frame able to leave, in milliseconds.
    pub(crate) stall_ms: u64,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            partitions: 1,
            retain_ms: 60_000,
            stall_ms: 30_000,
        }
    }
}

/// What a test reaches its running core through, beside its role address.
#[derive(Debug)]
pub(crate) struct CoreUnderTest {
    /// Where the core listens.
    pub(crate) address: Address,
    /// The closes the role's shell delivers to the core.
    pub(crate) closes: Sender<PartitionMessage>,
    /// Each higher discard version the core raises.
    pub(crate) discards: Receiver<u64>,
    /// The core's inbox, which its connections hand their requests to.
    pub(crate) inbox: UnboundedSender<CoreMessage>,
    /// What stops the core, as the role's stopping does; the test's end sets it too.
    pub(crate) stopping: Stopping,
    /// The core as its `Debug` shows it.
    pub(crate) debug: String,
}

impl CoreUnderTest {
    /// A connection to the core, dialled and authenticated as compute dials one.
    ///
    /// # Errors
    ///
    /// A [`TestError`] when the dial fails.
    pub(crate) async fn dial(&self) -> TestResult<RoleClient> {
        Ok(RoleClient::dial(&self.address, CLUSTER, "compute", PASSWORD).await?)
    }
}

/// The role cluster of the tests, its secret read from a file in `dir`.
pub(crate) fn role_cluster(dir: &std::path::Path) -> TestResult<RoleCluster> {
    std::fs::create_dir_all(dir)?;
    let file = dir.join("role-secret");
    std::fs::write(&file, PASSWORD)?;
    let secret = RoleSecret::read(&file.display().to_string())?;
    Ok(RoleCluster::new(CLUSTER, secret, Duration::from_secs(1)))
}

/// A loopback address of a port that was free a moment ago.
pub(crate) fn free_address() -> TestResult<String> {
    let port = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    Ok(format!("tcp://127.0.0.1:{port}"))
}

/// Runs `test` beside a core set up as `setup`, the frontend at `frontend`, until `test` ends; then the core stops, as a role
/// stopping stops it: `test`'s outcome, or the core's failure.
pub(crate) fn on_core(
    name: &str,
    setup: Setup,
    frontend: Address,
    test: impl AsyncFnOnce(CoreUnderTest) -> TestResult,
) -> TestResult {
    let dir = TestDir::new(name);
    Runtime::new()?.block_on(async {
        let placement = PlacementMap::even(setup.partitions, NodeId::new(1))?;
        let id = PartitionId::new(PARTITION)?;
        let (wanted, _wants) = unbounded();
        let blocks = RefillBlocks::new(leased()?, wanted);
        let partition = open_partition(dir.path(), id, &placement, blocks)?;
        let (host, remote, own) = host_of(&partition, dir.path()).await?;
        let address = Address::parse(&free_address()?)?;
        let (inbox_tx, inbox) = unbounded();
        let cluster = role_cluster(dir.path())?;
        accept(&address, &cluster, inbox_tx.clone()).await?;
        let (asks, _asked) = unbounded();
        let (closes_tx, closes) = std::sync::mpsc::channel();
        let (discards_tx, discards) = std::sync::mpsc::channel();
        let (answers, answered) = unbounded();
        spawn(forward_answers(answered, own.clone())).detach();
        let peers = Peers::new(cluster, frontend, "storage".to_string(), answers);
        let channels = CoreChannels::new(inbox, remote, own, closes, asks, discards_tx);
        // The core's clock is the runtime's: its timer's first tick is now.
        let clock = compio::time::interval(Duration::from_secs(1)).tick().await;
        let retention = Retention::new(setup.retain_ms);
        let parts = CoreParts::new(host, id, CLUSTER, clock, channels, peers, retention)
            .with_scan_stall_ms(setup.stall_ms)
            .with_tenants([TENANT]);
        let core = PartitionCore::new(parts);
        let stopping = Stopping::default();
        let reached = CoreUnderTest {
            address,
            closes: closes_tx,
            discards,
            inbox: inbox_tx,
            stopping: stopping.clone(),
            debug: format!("{core:?}"),
        };
        let tested = async {
            let outcome = test(reached).await;
            stopping.set();
            outcome
        };
        let (ran, tested) = futures::future::join(core.run(&stopping), tested).await;
        tested?;
        Ok(ran?)
    })
}

/// [`on_core`] on a thread of its own, `test` given at most [`TEST_DUE`] from its start to its end: a core spinning in a loop that
/// never yields fails the test at the deadline instead of holding it forever, and its thread ends with the test's process. The
/// core's setup and its stop are awaited without a bound, as [`on_core`] awaits them, and their failure is the outcome as there.
pub(crate) fn on_core_in_time(
    name: &'static str,
    setup: Setup,
    frontend: Address,
    test: impl AsyncFnOnce(CoreUnderTest) -> TestResult + Send + 'static,
) -> TestResult {
    // Nothing is sent on `begun`: its drop ends the setup, whether the core is set up and `test` begins or the setup fails.
    let (begun, began) = std::sync::mpsc::channel::<Infallible>();
    let (ended, tested) = std::sync::mpsc::channel();
    let (done, outcome) = std::sync::mpsc::channel();
    ThreadExecutor::new().spawn(move || {
        let timed = async move |core| {
            drop(begun);
            // Past the deadline no one receives the test's result: the test has failed already.
            drop(ended.send(test(core).await));
            Ok(())
        };
        drop(done.send(on_core(name, setup, frontend, timed)));
    });
    let Err(RecvError) = began.recv();
    let tested = match tested.recv_timeout(TEST_DUE) {
        Ok(tested) => tested,
        Err(RecvTimeoutError::Timeout) => {
            return Err(TestError::new(format!(
                "{name}: the test did not end within {TEST_DUE:?} of its core's setup"
            )));
        }
        // The setup failed, its error the outcome, or the thread panicked and ends without one.
        Err(RecvTimeoutError::Disconnected) => Ok(()),
    };
    let outcome = outcome.recv().map_err(|RecvError| {
        TestError::new(format!("{name}: its thread ended without an outcome"))
    })?;
    tested.and(outcome)
}

/// The host of `partition` over a log in `dir`, its mailbox reaching itself alone: the host, what it sends beyond its process, and
/// where messages reach it.
async fn host_of<'a>(
    partition: &'a StoragePartition,
    dir: &std::path::Path,
) -> TestResult<(
    StorageHost<'a>,
    Receiver<RemoteMessage>,
    Sender<PartitionMessage>,
)> {
    let id = partition.id();
    let (log, _) = Wal::open(
        CompioStorage,
        dir.join("wal"),
        WalConfig::new(CLUSTER, false),
    )
    .await?;
    let pipeline = Pipeline::new(partition.applier(), log, RoundBounds::new(1 << 20, 1 << 10))?
        .with_partition(id.get(), Epoch::ZERO, CloseRequest::default(), &[])
        .bound(id, partition.placement());
    let (remote_tx, remote) = std::sync::mpsc::channel();
    let mailbox = PartitionMailbox::mesh(&[id])
        .into_iter()
        .next()
        .context("the partition's mailbox")?
        .with_remote(remote_tx);
    let own = mailbox.sender(id).context("its mailbox reaches itself")?;
    let host = PartitionHost::new(partition, pipeline, mailbox).with_first_ticket(1);
    Ok((host, remote, own))
}

/// Hands each answer a home in another process sent to the partition's own mailbox, as the role's shell does.
async fn forward_answers(
    mut answered: UnboundedReceiver<PartitionMessage>,
    own: Sender<PartitionMessage>,
) {
    while let Some(answer) = answered.next().await {
        if own.send(answer).is_err() {
            return;
        }
    }
}

/// A block of each type the partition allocates.
fn leased() -> TestResult<LeasedBlocks> {
    let leased = LeasedBlocks::new();
    for prefix in [TypePrefix::RESOURCE, TypePrefix::STATEMENT] {
        let block = SequenceBlock::new(1, BLOCK).context("a block")?;
        leased.supply(prefix, block)?;
    }
    Ok(leased)
}

/// Partition `id` of `placement` over fjall in `dir`, as the storage role opens it.
fn open_partition(
    dir: &std::path::Path,
    id: PartitionId,
    placement: &PlacementMap,
    blocks: RefillBlocks,
) -> TestResult<StoragePartition> {
    let owned = StoragePartition::owned(id, placement, Epoch::ZERO).context("the map's ranges")?;
    let store_dir = dir.join("store");
    std::fs::create_dir_all(&store_dir)?;
    let store = FjallStore::open(&store_dir, Durability::Log)?;
    let cache = MokaListCache::new(CACHE_BYTES, |_: &[u8], list: &CachedList| list.weight());
    let lists = Lists::new(store, cache, ListsConfig::default());
    let quads = QuadStore::open(lists, blocks, RandomSource::new(1))?.owning(owned);
    let applier = ChangesetApplier::over(quads, ThreadExecutor::new(), 1)?;
    Ok(Partition::new(id, placement.clone(), applier))
}

/// Listens at `address`, each connection a session of the core reached through `core`.
async fn accept(
    address: &Address,
    cluster: &RoleCluster,
    core: UnboundedSender<CoreMessage>,
) -> TestResult {
    let listener = listen(&address.to_string()).await?;
    let credentials = cluster.secret().credentials()?;
    let server = Server::new(CLUSTER, "storage", Endpoint::Partition)?
        .with_welcome(|welcome| welcome.with_initial_credit(PARTITION_INITIAL_CREDIT))
        .with_iterations(ROLE_ITERATIONS);
    spawn(async move {
        let mut connection = 0u64;
        while let Ok(carrier) = listener.accept().await {
            connection += 1;
            let session = Yielding(StorageSession::new(connection, core.clone(), HELD_BYTES));
            let (server, credentials) = (server.clone(), credentials.clone());
            spawn(async move {
                serve_connection(carrier, &server, credentials, session).await;
            })
            .detach();
        }
    })
    .detach();
    Ok(())
}

/// An Entries frame of a Scan, decoded.
#[derive(Debug, Clone, Default)]
pub(crate) struct Batch {
    pub(crate) range: u32,
    pub(crate) through: u32,
    pub(crate) end: u8,
    pub(crate) entries: Vec<(Vec<u8>, Vec<u8>)>,
    pub(crate) last: bool,
}

/// An Error frame.
#[derive(Debug, Clone)]
pub(crate) struct Refused {
    pub(crate) stream: u64,
    pub(crate) code: ErrorCode,
    pub(crate) detail: u64,
    pub(crate) message: String,
}

/// The next frame `client` reads, within [`DUE`].
///
/// # Errors
///
/// A [`TestError`] when none comes in time or the read fails.
pub(crate) async fn next_frame(client: &mut RoleClient) -> TestResult<ReceivedFrame> {
    Ok(compio::time::timeout(DUE, client.next_frame())
        .await
        .context("a frame within its time")??)
}

/// `frame` as an Error frame, when it is one.
pub(crate) fn refusal_of(frame: &ReceivedFrame) -> TestResult<Option<Refused>> {
    let view = frame.view()?;
    if view.command() != Command::Error {
        return Ok(None);
    }
    let error = ErrorReplyView::from_frame(&view)?;
    Ok(Some(Refused {
        stream: view.header().stream(),
        code: error.code(),
        detail: error.detail(),
        message: error.message().to_string(),
    }))
}

/// Sends `scan` on `client` and reads its answer, a frame's credit granted after each frame: its Entries frames, each checked as
/// the dialling side checks it, and the refusal ending it, if one does.
///
/// # Errors
///
/// A [`TestError`] when a frame does not come in time, fails its check, or is of another command.
pub(crate) async fn scanned(
    client: &mut RoleClient,
    scan: &Scan,
) -> TestResult<(Vec<Batch>, Option<Refused>)> {
    client.send_only(scan).await?;
    let stream = client.last_stream();
    let mut sequence = EntriesSequence::new(scan.ranges());
    let mut batches = Vec::new();
    loop {
        let frame = next_frame(client).await?;
        if let Some(refused) = refusal_of(&frame)? {
            return Ok((batches, Some(refused)));
        }
        let view = frame.view()?;
        if view.command() != Command::Entries {
            return Err(TestError::new(format!(
                "a frame of command {}",
                view.command().number()
            )));
        }
        let entries = sequence.accept(&view)?;
        let batch = Batch {
            range: entries.range(),
            through: entries.through(),
            end: entries.end().number(),
            entries: entries
                .entries()
                .iter()
                .map(|entry| {
                    (
                        entry.key().to_vec(),
                        entry.value().unwrap_or_default().to_vec(),
                    )
                })
                .collect(),
            last: view.header().flags() & FLAG_LAST != 0,
        };
        let last = batch.last;
        batches.push(batch);
        if last {
            return Ok((batches, None));
        }
        client.send_on(stream, &Credit::new(1)).await?;
    }
}

/// The IRI `http://ex.org/<name>`.
pub(crate) fn ex(name: &str) -> String {
    format!("http://ex.org/{name}")
}

/// A Write inserting `<subject> <p> <object>` for each pair of `edges`, every name an IRI under `http://ex.org/`.
pub(crate) fn edges_write(edges: &[(&str, &str)]) -> Write {
    let mut table = TermTable::new();
    let predicate = table.iri(&ex("p"));
    let changes = edges
        .iter()
        .map(|(subject, object)| WriteChange::Insert {
            graph: None,
            subject: table.iri(&ex(subject)),
            predicate,
            object: table.iri(&ex(object)),
        })
        .collect();
    Write::new(table, changes)
}

/// Binds a read-write transaction of `identity` on `client`, as the frontend binds one: its stream and its read version.
///
/// # Errors
///
/// A [`TestError`] when the Bind is refused or its reply does not come in time.
pub(crate) async fn bind(client: &mut RoleClient, identity: u8) -> TestResult<(u64, u64)> {
    let bind = Bind::new(
        PARTITION,
        BindKind::Transaction,
        TENANT,
        0,
        PRINCIPAL,
        [identity; 16],
    );
    let reply = compio::time::timeout(DUE, client.request(&bind))
        .await
        .context("the Bind's reply")??;
    let read_version = BindView::from_frame(&reply.view()?)?.read_version();
    Ok((client.last_stream(), read_version))
}

/// Commits `write` in a transaction of its own on `client`, through no session: the commit's version.
///
/// # Errors
///
/// A [`TestError`] when any of its frames is refused or does not come in time.
pub(crate) async fn commit(client: &mut RoleClient, write: &Write) -> TestResult<u64> {
    let (stream, _) = bind(client, 0x77).await?;
    client.send_on(stream, write).await?;
    let held = next_frame(client).await?;
    if let Some(refused) = refusal_of(&held)? {
        return Err(TestError::new(format!(
            "the Write was refused: {refused:?}"
        )));
    }
    drop(held);
    client.send_on(stream, &Commit::new([0; 16], 0, 0)).await?;
    let answer = next_frame(client).await?;
    if let Some(refused) = refusal_of(&answer)? {
        return Err(TestError::new(format!(
            "the Commit was refused: {refused:?}"
        )));
    }
    Ok(CommitView::from_frame(&answer.view()?)?.commit_version())
}

/// The ID the partition at `client` holds for `iri`, read from its dictionary.
///
/// # Errors
///
/// A [`TestError`] when the dictionary holds none.
pub(crate) async fn id_of(client: &mut RoleClient, iri: &str) -> TestResult<SystemId> {
    let mut scan = Scan::new(ScanAt::Latest, 0, 0, PRINCIPAL, [0; 16], 0, 0);
    let key = term_key(TENANT, iri);
    let past = [key.as_slice(), &[0]].concat();
    scan.push_range(ScanRange::new(PARTITION, &key, &past));
    let (batches, refused) = scanned(client, &scan).await?;
    let (_, value) = batches
        .first()
        .and_then(|batch| batch.entries.first())
        .context("the dictionary's entry")?;
    assert!(refused.is_none(), "{refused:?}");
    Ok(SystemId::decode(value)?)
}

/// The key of the forward list of `<subject> <p>`, by the IDs the partition at `client` holds.
///
/// # Errors
///
/// A [`TestError`] when it holds no ID of either.
pub(crate) async fn forward_key(client: &mut RoleClient, subject: &str) -> TestResult<Vec<u8>> {
    Ok(QuadKey::Forward {
        tenant: TENANT,
        graph: Graph::Default,
        predicate: id_of(client, &ex("p")).await?,
        subject: id_of(client, &ex(subject)).await?,
    }
    .to_bytes())
}

/// The object encodings `value`, a list's object records, holds, in its order.
///
/// # Errors
///
/// A [`TestError`] when `value` is no list's records.
pub(crate) fn objects_of(value: &[u8]) -> TestResult<Vec<Vec<u8>>> {
    Ok(ObjectRecords::read(value)?
        .iter()
        .map(|record| record.encoding().to_vec())
        .collect())
}
