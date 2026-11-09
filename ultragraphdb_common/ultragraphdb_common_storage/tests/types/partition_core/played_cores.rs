/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Partition cores of one process over fjall and their logs in a temporary directory, as the storage role runs them, their
//! connections played by a test through the messages a session hands its core. One partition's core serves the test, partition 1's
//! unless the test names another; every other partition is a home of the same process, its core held back until the test lets
//! it answer.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use compio::runtime::Runtime;
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures::channel::oneshot;
use ultragraphdb_cluster_metadata::LeasedBlocks;
use ultragraphdb_cluster_partition::{
    Partition, PartitionError, PartitionHost, PartitionMailbox, PartitionMessage, RemoteMessage,
};
use ultragraphdb_cluster_placement::{NodeId, PartitionId, PlacementMap};
use ultragraphdb_common_role::{Answer, ParkedBytes, RoleCluster, RoleSecret};
use ultragraphdb_common_service::Stopping;
use ultragraphdb_common_storage::{
    CoreChannels, CoreMessage, CoreParts, FrontendAsk, LogIo, PartitionCore, Peers, RefillBlocks,
    Retention, StoragePartition, TimedStorage,
};
use ultragraphdb_core_ids::{Epoch, TypePrefix};
use ultragraphdb_core_mapping::RandomSource;
use ultragraphdb_protocol_transport::{Address, BufferPool, Framer};
use ultragraphdb_protocol_wire::{
    Bind, BindKind, BodyBuilder, BodyView, Command, ErrorReply, ErrorReplyView, FrameView, Header,
    MAX_FRAME_SIZE,
};
use ultragraphdb_store_apply::ChangesetApplier;
use ultragraphdb_store_kv::{Durability, FjallStore, MokaListCache, ThreadExecutor};
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_quads::{QuadStore, iri_bucket};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_tool_test::TestDir;
use ultragraphdb_traits_blocks::SequenceBlock;
use ultragraphdb_transactions_commit::{CloseRequest, Pipeline, RoundBounds};
use ultragraphdb_transactions_log::{CompioStorage, Wal, WalConfig};

/// The cluster every frame names.
pub(crate) const CLUSTER: u64 = 7;

/// The tenant the placement map lists a run for.
pub(crate) const TENANT: u64 = 5;

/// The principal every request acts for.
pub(crate) const PRINCIPAL: u64 = 3;

/// How long a frame or an answer that is due may take, past which a test fails naming what it waited for.
pub(crate) const DUE: Duration = Duration::from_secs(5);

/// The posting lists' cache of each partition.
const CACHE_BYTES: u64 = 1 << 20;

/// The sequences of each block a partition holds, of either type.
const BLOCK: u64 = 1 << 16;

/// What a test plays its connections through: the tested partition's core, the frontend's side of what it asks, and the gate of the homes.
#[derive(Debug)]
pub(crate) struct Driver {
    core: UnboundedSender<CoreMessage>,
    asks: UnboundedReceiver<FrontendAsk>,
    homes: Option<oneshot::Sender<()>>,
    placement: PlacementMap,
}

/// A connection to the tested partition, as its session counts and receives for it.
#[derive(Debug)]
pub(crate) struct Connection {
    id: u64,
    parked: ParkedBytes,
    pushes: UnboundedSender<(u64, Vec<u8>)>,
    pushed: UnboundedReceiver<(u64, Vec<u8>)>,
}

impl Connection {
    /// The connection's number.
    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    /// What it holds parked now.
    pub(crate) fn parked(&self) -> usize {
        self.parked.count()
    }

    /// The next frame the core pushed to it, and its stream, or a failure naming `what` it waited for.
    pub(crate) async fn pushed(&mut self, what: &str) -> TestResult<(u64, Vec<u8>)> {
        compio::time::timeout(DUE, self.pushed.next())
            .await
            .map_err(|_| TestError::new(format!("no frame within {DUE:?}: {what}")))?
            .context(what)
    }

    /// Whether a frame waits to leave now.
    pub(crate) fn holds_push(&mut self) -> bool {
        self.pushed.try_recv().is_ok()
    }
}

impl Driver {
    /// Opens connection `id`, which stops reading at `bound` bytes parked.
    pub(crate) fn connect(&self, id: u64, bound: usize) -> TestResult<Connection> {
        let (pushes, pushed) = unbounded();
        let parked = ParkedBytes::new(bound);
        self.core.unbounded_send(CoreMessage::Opened {
            connection: id,
            parked: parked.clone(),
        })?;
        Ok(Connection {
            id,
            parked,
            pushes,
            pushed,
        })
    }

    /// Hands the core `body` as a request of `connection` on `stream`, as a session does: where its answer comes.
    pub(crate) fn send<B: BodyBuilder>(
        &self,
        connection: &Connection,
        stream: u64,
        body: &B,
    ) -> TestResult<oneshot::Receiver<Answer>> {
        let frame = framed(body, stream)?;
        self.send_frame(connection, stream, frame)
    }

    /// Hands the core `frame` as a request of `connection` on `stream`: where its answer comes.
    pub(crate) fn send_frame(
        &self,
        connection: &Connection,
        stream: u64,
        frame: Vec<u8>,
    ) -> TestResult<oneshot::Receiver<Answer>> {
        let pool = BufferPool::new(1, MAX_FRAME_SIZE);
        let mut buffer = pool.acquire()?;
        buffer.restore(frame);
        let (done, answer) = oneshot::channel();
        self.core.unbounded_send(CoreMessage::Request {
            connection: connection.id,
            stream,
            frame: Framer::from_client(CLUSTER, MAX_FRAME_SIZE).finish(buffer),
            done,
            pushes: connection.pushes.clone(),
            max_frame: u32::try_from(MAX_FRAME_SIZE)?,
        })?;
        Ok(answer)
    }

    /// The answer to `body` sent as a request of `connection` on `stream`.
    pub(crate) async fn ask<B: BodyBuilder>(
        &self,
        connection: &Connection,
        stream: u64,
        body: &B,
    ) -> TestResult<Answer> {
        answered(self.send(connection, stream, body)?).await
    }

    /// Hands the core `message` as a connection does.
    pub(crate) fn tell(&self, message: CoreMessage) -> TestResult {
        Ok(self.core.unbounded_send(message)?)
    }

    /// The next thing the core asks of the frontend, or a failure naming `what` it waited for.
    pub(crate) async fn asked(&mut self, what: &str) -> TestResult<FrontendAsk> {
        compio::time::timeout(DUE, self.asks.next())
            .await
            .map_err(|_| TestError::new(format!("nothing asked within {DUE:?}: {what}")))?
            .context(what)
    }

    /// Lets every home's core run, which answers what the tested partition asked of it.
    pub(crate) fn release_homes(&mut self) {
        if let Some(homes) = self.homes.take() {
            // The homes gone have stopped already, with the test.
            let _sent: Result<(), ()> = homes.send(());
        }
    }

    /// The partition owning `iri` of the tenant.
    pub(crate) fn home_of(&self, iri: &str) -> TestResult<u32> {
        let bucket = iri_bucket(TENANT, iri).context("an IRI's bucket")?;
        Ok(self.placement.partition_of(bucket, Epoch::ZERO).get())
    }

    /// An IRI of the tenant that `partition` owns, `at` choosing among them.
    pub(crate) fn owned_by(&self, partition: u32, at: &str) -> TestResult<String> {
        (0..100_000)
            .map(|n| format!("http://ex.org/{at}-{n}"))
            .find(|iri| {
                iri_bucket(TENANT, iri).is_some_and(|bucket| {
                    self.placement.partition_of(bucket, Epoch::ZERO).get() == partition
                })
            })
            .context("an IRI of the partition")
    }
}

/// The answer that comes on `answer`, or a failure past [`DUE`].
pub(crate) async fn answered(answer: oneshot::Receiver<Answer>) -> TestResult<Answer> {
    compio::time::timeout(DUE, answer)
        .await
        .map_err(|_| TestError::new(format!("no answer within {DUE:?}")))?
        .map_err(|_| TestError::new("the core dropped the request"))
}

/// A Bind of `kind` of the tenant on partition 1, its transaction named `identity`.
pub(crate) fn bind(kind: BindKind, identity: u8) -> Bind {
    Bind::new(1, kind, TENANT, 0, PRINCIPAL, [identity; 16])
}

/// `frame`, checked as its reader checks it: its command.
pub(crate) fn command_of(frame: &[u8]) -> TestResult<Command> {
    Ok(FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE)?.command())
}

/// The reply `answer` carries.
pub(crate) fn reply_of(answer: Answer) -> TestResult<Vec<u8>> {
    match answer {
        Answer::Reply(frame) => Ok(frame),
        Answer::Open | Answer::Done | Answer::Refuse(_) => {
            Err(TestError::new(format!("a reply, not {answer:?}")))
        }
    }
}

/// The refusal `answer` carries.
pub(crate) fn refusal_of(answer: Answer) -> TestResult<ErrorReply> {
    match answer {
        Answer::Refuse(refusal) => Ok(refusal),
        Answer::Reply(_) | Answer::Open | Answer::Done => {
            Err(TestError::new(format!("a refusal, not {answer:?}")))
        }
    }
}

/// The Error `frame` is, as its code, `detail` and message, or `None` for a frame of another command.
pub(crate) fn error_in(frame: &[u8]) -> TestResult<Option<ErrorReply>> {
    let view = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE)?;
    if view.command() != Command::Error {
        return Ok(None);
    }
    let error = ErrorReplyView::from_frame(&view)?;
    Ok(Some(
        ErrorReply::new(error.code(), error.message()).with_detail(error.detail()),
    ))
}

/// The frame of `body` on `stream`, the first a connection's framer expects.
pub(crate) fn framed<B: BodyBuilder>(body: &B, stream: u64) -> TestResult<Vec<u8>> {
    Ok(body.frame(
        Header::new(CLUSTER, B::COMMAND)
            .with_stream(stream)
            .with_sequence(1),
    )?)
}

/// Runs `test` against partition 1's core of `partitions`, each partition owning an even share of the buckets, the homes held
/// until the test releases them. The cores stop once the test ends.
pub(crate) fn served<F, Fut>(partitions: u32, test: F) -> TestResult
where
    F: FnOnce(Driver) -> Fut,
    Fut: Future<Output = TestResult>,
{
    served_at(partitions, 1, test)
}

/// As [`served`], against partition `tested`'s core, every other partition a home.
pub(crate) fn served_at<F, Fut>(partitions: u32, tested: u32, test: F) -> TestResult
where
    F: FnOnce(Driver) -> Fut,
    Fut: Future<Output = TestResult>,
{
    let dir = TestDir::new("played-cores");
    let at = usize::try_from(tested)?
        .checked_sub(1)
        .context("a partition numbered from 1")?;
    Runtime::new()?.block_on(serve(dir.path(), (partitions, at), test))
}

/// A home in another process, played by a test: the registrations the tested partition sends it, and the partition's mailbox,
/// where its answers go.
#[derive(Debug)]
pub(crate) struct RemoteHome {
    registers: std::sync::mpsc::Receiver<RemoteMessage>,
    answers: std::sync::mpsc::Sender<PartitionMessage>,
}

impl RemoteHome {
    /// The asking write's ticket and each registration's index of the next registrations sent to the home, or a failure past [`DUE`].
    pub(crate) async fn registered(&self) -> TestResult<(u64, Vec<usize>)> {
        let polled = async {
            loop {
                match self.registers.try_recv() {
                    Ok(RemoteMessage::Register { write, batch, .. }) => {
                        return Ok((write, batch.into_iter().map(|(index, _)| index).collect()));
                    }
                    Ok(RemoteMessage::Answered { .. }) => {}
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        // The core shares this thread: it runs while the test waits.
                        compio::time::sleep(Duration::from_millis(10)).await;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        return Err(TestError::new("the partition's mailbox is gone"));
                    }
                }
            }
        };
        compio::time::timeout(DUE, polled)
            .await
            .map_err(|_| TestError::new(format!("no registration within {DUE:?}")))?
    }

    /// Answers registration `index` of the write `write` with a refusal of `outcome`, as a home refusing it does.
    pub(crate) fn refuse(&self, write: u64, index: usize, outcome: u16) -> TestResult {
        self.answers.send(PartitionMessage::Answered {
            write,
            index,
            answer: Err(PartitionError::refused(outcome)),
        })?;
        Ok(())
    }
}

/// Runs `test` against partition 1's core of a map of two partitions, partition 2 a home in another process that `test` plays.
/// The core stops once the test ends.
pub(crate) fn served_beside_remote<F, Fut>(test: F) -> TestResult
where
    F: FnOnce(Driver, RemoteHome) -> Fut,
    Fut: Future<Output = TestResult>,
{
    let dir = TestDir::new("played-remote");
    Runtime::new()?.block_on(serve_beside_remote(dir.path(), test))
}

async fn serve_beside_remote<F, Fut>(root: &Path, test: F) -> TestResult
where
    F: FnOnce(Driver, RemoteHome) -> Fut,
    Fut: Future<Output = TestResult>,
{
    let placement = PlacementMap::even(2, NodeId::new(1))?;
    let id = PartitionId::new(1)?;
    // The core's clock is the runtime's: its timer's first tick is now.
    let clock = compio::time::interval(Duration::from_secs(1)).tick().await;
    let cluster = role_cluster(root)?;
    let dir = root.join("p1");
    let partition = open_partition(&dir, id, &placement)?;
    let (registers_to, registers) = std::sync::mpsc::channel();
    // A mesh of the tested partition alone: what it sends partition 2 leaves the process, to the test.
    let mailbox = PartitionMailbox::mesh(&[id])
        .into_iter()
        .next()
        .context("the partition's mailbox")?
        .with_remote(registers_to);
    let answers = mailbox.sender(id).context("a mailbox reaching itself")?;
    let (core, inbox, asks) =
        core_of(&partition, mailbox, &dir.join("wal"), (&cluster, clock)).await?;
    let stopping = Stopping::default();
    let driver = Driver {
        core: inbox,
        asks,
        homes: None,
        placement,
    };
    let tested = async {
        let tested = test(driver, RemoteHome { registers, answers }).await;
        stopping.set();
        tested
    };
    let (served, tested) = futures::future::join(core.run(&stopping), tested).await;
    served?;
    tested
}

async fn serve<F, Fut>(root: &Path, (partitions, at): (u32, usize), test: F) -> TestResult
where
    F: FnOnce(Driver) -> Fut,
    Fut: Future<Output = TestResult>,
{
    let placement = PlacementMap::even(partitions, NodeId::new(1))?;
    let ids = (1..=partitions)
        .map(PartitionId::new)
        .collect::<Result<Vec<_>, _>>()?;
    // The cores' clock is the runtime's: its timer's first tick is now.
    let clock = compio::time::interval(Duration::from_secs(1)).tick().await;
    let cluster = role_cluster(root)?;
    let opened = ids
        .iter()
        .map(|id| open_partition(&root.join(format!("p{}", id.get())), *id, &placement))
        .collect::<TestResult<Vec<_>>>()?;
    let mut cores = Vec::new();
    let mut inboxes = Vec::new();
    let mut asks = Vec::new();
    for ((partition, mailbox), dir) in opened.iter().zip(PartitionMailbox::mesh(&ids)).zip(
        ids.iter()
            .map(|id| root.join(format!("p{}", id.get())).join("wal")),
    ) {
        let (core, inbox, ask) = core_of(partition, mailbox, &dir, (&cluster, clock)).await?;
        cores.push(core);
        inboxes.push(inbox);
        asks.push(ask);
    }
    if at >= cores.len() {
        return Err(TestError::new("the tested partition is one of the map's"));
    }
    let first = cores.remove(at);
    // Every inbox stays open while the cores run, as a role's listener holds each: a core whose inbox ended would turn without
    // waiting, and starve the runtime it shares with the others.
    let (core, asks) = (
        inboxes
            .get(at)
            .cloned()
            .context("the tested partition's inbox")?,
        asks.remove(at),
    );
    let (gate, released) = oneshot::channel();
    let stopping = Stopping::default();
    let driver = Driver {
        core,
        asks,
        homes: Some(gate),
        placement: placement.clone(),
    };
    let homes = async {
        match released.await {
            Ok(()) => {
                futures::future::join_all(cores.into_iter().map(|home| home.run(&stopping))).await
            }
            // A test ending without releasing them stops them before they ran.
            Err(oneshot::Canceled) => Vec::new(),
        }
    };
    let tested = async {
        let tested = test(driver).await;
        stopping.set();
        tested
    };
    let (served, (homes, tested)) =
        futures::future::join(first.run(&stopping), futures::future::join(homes, tested)).await;
    served?;
    for home in homes {
        home?;
    }
    tested
}

/// The cluster's identity and the secret its roles share, read from a file in `root`.
fn role_cluster(root: &Path) -> TestResult<RoleCluster> {
    std::fs::create_dir_all(root)?;
    let file = root.join("secret");
    std::fs::write(&file, "s3cret")?;
    let secret = RoleSecret::read(&file.display().to_string())?;
    Ok(RoleCluster::new(CLUSTER, secret, DUE))
}

/// Partition `id` of `placement`, its store in `dir`, holding a block of either type's sequences.
fn open_partition(
    dir: &Path,
    id: PartitionId,
    placement: &PlacementMap,
) -> TestResult<StoragePartition> {
    let leased = LeasedBlocks::new();
    for (prefix, start) in [(TypePrefix::RESOURCE, 1), (TypePrefix::STATEMENT, 1)] {
        let block = SequenceBlock::new(start, start + BLOCK).context("a block")?;
        leased.supply(prefix, block)?;
    }
    // Nothing leases ahead: the block held is far more than a test takes.
    let (wanted, _ignored) = unbounded();
    let blocks = RefillBlocks::new(leased, wanted);
    let owned = StoragePartition::owned(id, placement, Epoch::ZERO).context("the map's ranges")?;
    let store_dir = dir.join("store");
    std::fs::create_dir_all(&store_dir)?;
    let store = FjallStore::open(&store_dir, Durability::Log)?;
    let cache = MokaListCache::new(CACHE_BYTES, |_: &[u8], list: &CachedList| list.weight());
    let lists = Lists::new(store, cache, ListsConfig::default());
    let quads =
        QuadStore::open(lists, blocks, RandomSource::new(u64::from(id.get())))?.owning(owned);
    let applier = ChangesetApplier::over(quads, ThreadExecutor::new(), 1)?;
    Ok(Partition::new(id, placement.clone(), applier))
}

/// The core of `partition`, its log in `dir`, reaching the others through `mailbox`: it, the sender to its inbox, and what it
/// asks of the frontend.
async fn core_of<'a>(
    partition: &'a StoragePartition,
    mailbox: PartitionMailbox,
    dir: &PathBuf,
    (cluster, clock): (&RoleCluster, Instant),
) -> TestResult<(
    PartitionCore<'a>,
    UnboundedSender<CoreMessage>,
    UnboundedReceiver<FrontendAsk>,
)> {
    let id = partition.id();
    let io = LogIo::default();
    let storage = TimedStorage::new(CompioStorage, io.clone(), clock);
    let (log, _) = Wal::open(storage, dir, WalConfig::new(CLUSTER, false)).await?;
    let pipeline = Pipeline::new(
        partition.applier(),
        log,
        RoundBounds::new(u32::MAX, u32::MAX),
    )?
    .with_partition(id.get(), Epoch::ZERO, CloseRequest::default(), &[])
    .bound(id, partition.placement());
    let own = mailbox.sender(id).context("a mailbox reaching itself")?;
    let host = PartitionHost::new(partition, pipeline, mailbox);
    let (to_core, inbox) = unbounded();
    let (asks_to, asks) = unbounded();
    let (_remote_to, remote) = std::sync::mpsc::channel();
    let (_closes_to, closes) = std::sync::mpsc::channel();
    let (discards, _raised) = std::sync::mpsc::channel();
    let (answers, _answered) = unbounded();
    let frontend = Address::parse("tcp://127.0.0.1:1")?;
    let peers = Peers::new(cluster.clone(), frontend, "storage".to_string(), answers);
    let channels = CoreChannels::new(inbox, remote, own, closes, asks_to, discards);
    let parts = CoreParts::new(host, id, CLUSTER, clock, channels, peers, Retention::new(0))
        .with_tenants([TENANT])
        .with_log_io(io);
    Ok((PartitionCore::new(parts), to_core, asks))
}
