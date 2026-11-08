/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use compio::runtime::spawn;
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures::channel::oneshot;
use ultragraphdb_cluster_metadata::LeasedBlocks;
use ultragraphdb_cluster_partition::{
    Partition, PartitionHost, PartitionMailbox, PartitionMessage, RemoteMessage,
};
use ultragraphdb_cluster_placement::{NodeId, PartitionId, PlacementMap};
use ultragraphdb_cluster_sequencer::{CatalogVersion, Close};
use ultragraphdb_common_role::{
    PARTITION_INITIAL_CREDIT, ROLE_ITERATIONS, RoleClient, RoleCluster, RoleCredentials, Server,
    frontend_address, listen, retry_until, serve_connection,
};
use ultragraphdb_common_service::{
    AcceptPace, Readiness, RoleKind, RoleStart, Stopping, core_runtime,
};
use ultragraphdb_common_service_config::{ServiceConfig, StorageTable, partition_address};
use ultragraphdb_common_storage::{
    CoreChannels, CoreEnds, CoreMessage, CoreParts, FrontendAsk, HeldCatalogs, PartitionCore,
    Peers, RefillBlocks, Retention, SCAN_STALL_MS_DEFAULT, StorageHost, StoragePartition,
    StorageSession,
};
use ultragraphdb_core_catalog::CatalogContents;
use ultragraphdb_core_ids::{Epoch, Tenant, TypePrefix, Version};
use ultragraphdb_core_mapping::RandomSource;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::{Address, Endpoint, TransportError, TransportErrorEnum};
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_protocol_wire::{
    BodyView, Catalog, CatalogView, CloseRequest as CloseRequestBody, Command, DefaultEntry,
    EpochCloseView, ErrorCode, ErrorReplyView, Join, Lease, LeaseView, Placement, PlacementView,
    Report, Subscribe,
};
use ultragraphdb_store_apply::ChangesetApplier;
use ultragraphdb_store_kv::{Durability, FjallStore, MokaListCache, ThreadExecutor};
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_quads::QuadStore;
use ultragraphdb_traits_blocks::SequenceBlock;
use ultragraphdb_traits_catalog::HeldVersion;
use ultragraphdb_transactions_commit::{CloseRequest, Pipeline, Replay, RoundBounds, Sessions};
use ultragraphdb_transactions_log::{CompioStorage, Wal, WalConfig};

/// The posting lists' cache per partition.
const CACHE_BYTES: u64 = 64 << 20;

/// The sequences a core leases at a time, and how many blocks it keeps ahead.
const BLOCK_SIZE: u64 = 1 << 16;
const BLOCKS_AHEAD: usize = 2;

const POLL: std::time::Duration = std::time::Duration::from_millis(20);

/// How long a report left unanswered waits before it is sent again.
const REPORT_AGAIN: std::time::Duration = std::time::Duration::from_millis(100);

/// The replica a partition's directory: one per partition until stage 8's replication.
const REPLICA: u32 = 1;

#[expect(
    clippy::disallowed_methods,
    reason = "the shell reads the clock and tells the partition's host, which reads none"
)]
fn now() -> Instant {
    Instant::now()
}

#[expect(
    clippy::disallowed_methods,
    reason = "the shell reads the wall clock once per start, to number tickets above every ticket before"
)]
fn first_ticket() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        // Microseconds since 1970 fit a `u64` until the year 586,912, so the cast loses nothing.
        .map_or(1, |since| since.as_micros() as u64)
}

/// What every partition core of the process reads at start.
#[derive(Debug, Clone)]
struct CoreStart {
    config: ServiceConfig,
    cluster: RoleCluster,
    frontend: Address,
    base: String,
    first: u32,
    data_dir: PathBuf,
    core: usize,
    id: PartitionId,
}

/// The storage role: a thread and a runtime per partition, each recovering, listening and joining for itself; it is ready
/// once all of them are, and at a stop each flushes what its log holds through its store.
///
/// # Errors
///
/// [`RoleError::configuration`] for a setting absent or out of its bounds or a refused role secret;
/// [`RoleError::unavailable`] when a thread cannot start or the frontend does not take the role by its deadline;
/// [`RoleError::io`] for a failed store or log.
///
/// A future dropped at any await leaves the partitions' threads running, each ending at the service's stop, which no
/// one then waits for.
pub(crate) async fn storage_role(
    start: RoleStart,
    readiness: Readiness,
    stopping: Stopping,
) -> Result<(), RoleError> {
    let cluster = RoleCluster::read(&start)?;
    let config = start.config().clone();
    let table = config.storage();
    let required = |value: Option<&str>, setting: &str| -> Result<String, RoleError> {
        value
            .map(str::to_string)
            .ok_or_else(|| RoleError::configuration(format!("the setting {setting} is required")))
    };
    let base = required(table.role_address(), "storage.role_address")?;
    let data_dir = PathBuf::from(required(table.data_dir(), "storage.data_dir")?);
    let first = table.partition_first().unwrap_or(1);
    let count = table
        .partition_count()
        .filter(|count| *count > 0)
        .ok_or_else(|| {
            RoleError::configuration("the setting storage.partition_count is at least 1")
        })?;
    let frontend = frontend_address(&start)?;
    let ids = (0..count)
        .map(|at| {
            first
                .checked_add(at)
                .and_then(|partition| PartitionId::new(partition).ok())
                .ok_or_else(|| {
                    RoleError::configuration(
                        "the setting storage.partition_first is out of its bounds",
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut mailboxes = PartitionMailbox::mesh(&ids);
    let mut ended = CoreEnds::default();
    let mut ready = Vec::new();
    for (core, id) in ids.iter().enumerate().rev() {
        let mailbox = mailboxes
            .pop()
            .ok_or_else(|| RoleError::software("a mailbox per partition"))?;
        let core_start = CoreStart {
            config: config.clone(),
            cluster: cluster.clone(),
            frontend: frontend.clone(),
            base: base.clone(),
            first,
            data_dir: data_dir.clone(),
            core,
            id: *id,
        };
        let (joined, is_ready) = oneshot::channel();
        ready.push(is_ready);
        ended.add(spawn_core(core_start, mailbox, joined, stopping.clone())?);
    }
    for is_ready in ready {
        match wait_or_end(is_ready, &mut ended, &stopping).await {
            Ok(true) => {}
            Ok(false) => return ended.finish().await,
            Err(failure) => return stop_the_rest(failure, ended, &stopping).await,
        }
    }
    readiness.set(true);
    tracing::info!(partitions = count, "ready");
    // An event loop: the service's stop or a core's end ends it (Ultra rule 2).
    while !stopping.is_stopping() {
        if let Err(failure) = ended.look() {
            return stop_the_rest(failure, ended, &stopping).await;
        }
        compio::time::sleep(POLL).await;
    }
    ended.finish().await
}

/// A partition failed, so the process shuts down through its ordinary path: every other partition flushes what its log holds, and
/// the process exits with the failure's code.
///
/// A future dropped while it waits leaves the stop set and the partitions flushing on their threads, unwaited.
async fn stop_the_rest(
    failure: RoleError,
    ended: CoreEnds,
    stopping: &Stopping,
) -> Result<(), RoleError> {
    tracing::error!(%failure, "a partition stopped; stopping the others");
    stopping.set();
    // The first failure is the one reported.
    drop(ended.finish().await);
    tracing::info!(role = %RoleKind::Storage, "stopping");
    Err(failure)
}

/// Whether the core behind `is_ready` became ready; false once the service stops first. A core ending first ends this.
///
/// A future dropped at its wait holds nothing: a core then telling its readiness finds no one, which it takes as a role
/// gone.
async fn wait_or_end(
    mut is_ready: oneshot::Receiver<()>,
    ended: &mut CoreEnds,
    stopping: &Stopping,
) -> Result<bool, RoleError> {
    loop {
        if matches!(is_ready.try_recv(), Ok(Some(()))) {
            return Ok(true);
        }
        if ended.look()? || stopping.is_stopping() {
            return Ok(false);
        }
        compio::time::sleep(POLL).await;
    }
}

fn spawn_core(
    start: CoreStart,
    mailbox: PartitionMailbox,
    joined: oneshot::Sender<()>,
    stopping: Stopping,
) -> Result<oneshot::Receiver<Result<(), RoleError>>, RoleError> {
    let (end, ended) = oneshot::channel();
    #[expect(
        clippy::disallowed_methods,
        reason = "a partition's core is a real thread, which no simulation replays: its state machines are simulated beneath it"
    )]
    let spawned = std::thread::Builder::new()
        .name(format!("partition-{}", start.id.get()))
        .spawn(move || {
            let outcome = core_runtime(&start.config, start.core).and_then(|(runtime, cpu)| {
                tracing::info!(partition = start.id.get(), "{cpu}");
                runtime.block_on(serve_core(start, mailbox, joined, stopping))
            });
            // A role gone has no one to tell.
            drop(end.send(outcome));
        });
    let _thread = spawned
        .map_err(|error| RoleError::unavailable(format!("a partition's thread: {error}")))?;
    Ok(ended)
}

/// A partition's whole life on its core: the map it asks for, its store and log recovered, then its service.
///
/// Its thread's `block_on` runs it to its end; a future dropped at an await would drop the partition, its log, as its
/// last sync left it, and the discard thread's sender.
async fn serve_core(
    start: CoreStart,
    mailbox: PartitionMailbox,
    joined: oneshot::Sender<()>,
    stopping: Stopping,
) -> Result<(), RoleError> {
    let name = RoleKind::Storage.to_string();
    let cluster = &start.cluster;
    let Some((placement, tenants)) = retry_until(cluster, &name, &stopping, || {
        placement_of(&start.frontend, cluster, &name)
    })
    .await?
    else {
        return Ok(());
    };
    let own = start.id;
    let dir = start
        .data_dir
        .join(format!("p{}", own.get()))
        .join(format!("r{REPLICA}"));
    std::fs::create_dir_all(&dir)
        .map_err(|error| RoleError::io(format!("the data directory {}: {error}", dir.display())))?;
    let leased = LeasedBlocks::new();
    // One want per lease the store asks for, which it asks at each allocation while it holds no block ahead: the
    // frontend's task drains them as it runs, and nothing bounds them while it waits on the frontend.
    let (wanted, wants) = unbounded();
    let blocks = RefillBlocks::new(leased.clone(), wanted);
    let partition = Arc::new(open_partition(&dir.join("store"), own, &placement, blocks)?);
    // At most one ask per core turn that raises the bound, each pass of the discard thread draining every ask: it holds
    // the turns of one synced write.
    let (discards, raised) = std::sync::mpsc::channel();
    let discarding = spawn_discards(own, Arc::clone(&partition), raised)?;
    let config = WalConfig::new(cluster.id(), false);
    let bounds = RoundBounds::new(config.round_bytes(), config.round_transactions());
    let (mut log, _) = Wal::open(CompioStorage, dir.join("wal"), config)
        .await
        .map_err(|error| RoleError::io(format!("the partition {} log: {error}", own.get())))?;
    // A frontend that is not there yet, or stops under the start, is asked again until the join deadline, as joining is.
    let Some(held) = retry_until(cluster, &name, &stopping, || {
        held_versions(&start.frontend, cluster, &name, &tenants)
    })
    .await?
    else {
        return Ok(());
    };
    let applied = replayed(&partition, &mut log, &held, &tenants).await?;
    let delivered = last_close(applied);
    let (in_effect, asked) = (
        versions_held(&partition, &tenants)?,
        close_request(&start.config)?,
    );
    let pipeline = Pipeline::new(partition.applier(), log, bounds)
        .map_err(|error| RoleError::io(format!("the pipeline: {error}")))?
        .with_partition(own.get(), delivered, asked, &in_effect)
        .bound(own, partition.placement());
    // What the mailbox hands beyond the process, bounded as a mailbox's inbox is (`PartitionMailbox::mesh`): the writes
    // parked here, each a batch a second until its limit, and the answers to the Registrations taken; the core drains it
    // each turn.
    let (remote_tx, remote) = std::sync::mpsc::channel();
    let mailbox = mailbox.with_remote(remote_tx);
    let self_sender = mailbox
        .sender(own)
        .ok_or_else(|| RoleError::software("a partition's mailbox reaches itself"))?;
    let host = PartitionHost::new(&partition, pipeline, mailbox).with_first_ticket(first_ticket());
    let served = Serving {
        host,
        remote,
        self_sender,
        leased,
        wants,
        delivered,
        joined,
        discards,
        tenants,
    };
    let outcome = serve_partition(&start, served, &stopping).await;
    // The core is gone, and its sender with it, so the thread ends past the write it may be in.
    discarding
        .join()
        .map_err(|_| RoleError::software("a partition's discard thread panicked"))?;
    outcome
}

/// `partition`'s catalog versions loaded and its log replayed over them: the version it applied through.
///
/// A future dropped at the replay leaves the store holding the rounds applied so far, which a recovery replays again
/// from the log.
async fn replayed(
    partition: &StoragePartition,
    log: &mut Wal<CompioStorage>,
    held: &HeldCatalogs,
    tenants: &[u64],
) -> Result<u64, RoleError> {
    let own = partition.id();
    load_catalogs(partition.applier(), held, tenants)?;
    let replay = Replay::run_with_catalog(
        own.get(),
        partition.applier(),
        log,
        Sessions::DEFAULT_CAPACITY,
        held,
    )
    .await
    .map_err(|error| RoleError::io(format!("the partition {} replay: {error}", own.get())))?;
    tracing::info!(partition = own.get(), rounds = replay.rounds(), "recovered");
    Ok(replay.applied())
}

/// Loads into `applier` the catalog versions of `tenants` in effect at its store's applied epoch, each below the rounds a replay runs:
/// a version whose round lies below the log's checkpoint is taken effect by nothing else. One naming what the dictionary does not
/// hold never took effect here, and neither did those after it.
fn load_catalogs(
    applier: &ChangesetApplier<FjallStore, MokaListCache<CachedList>, ThreadExecutor, RefillBlocks>,
    held: &HeldCatalogs,
    tenants: &[u64],
) -> Result<(), RoleError> {
    let applied = Version::from_u64(applier.quads().lists().applied_ts())
        .epoch()
        .get();
    for tenant in tenants {
        for version in held.up_to(*tenant, applied) {
            let contents = CatalogContents::decode(*tenant, version.version(), version.contents())
                .map_err(|error| RoleError::software(format!("a catalog version: {error}")))?;
            if let Err(error) = applier.load_catalog(&contents) {
                tracing::warn!(%error, version = version.version(), "a catalog version did not take effect here");
                break;
            }
        }
    }
    Ok(())
}

/// Each of `tenants`' catalog versions `partition` holds in effect once it replayed, which its commit step starts from.
fn versions_held(
    partition: &StoragePartition,
    tenants: &[u64],
) -> Result<Vec<CatalogVersion>, RoleError> {
    tenants
        .iter()
        .map(|tenant| {
            let catalog = partition.applier().catalog(*tenant).map_err(|error| {
                RoleError::software(format!("the catalog of tenant {tenant}: {error}"))
            })?;
            Ok(CatalogVersion::new(Tenant::new(*tenant), catalog.version()))
        })
        .collect()
}

/// A thread raising the discard version of `partition` as `raised` says: the store's write of it syncs, which no read of the core
/// waits for. Asks arriving meanwhile come to one, the latest.
fn spawn_discards(
    id: PartitionId,
    partition: Arc<StoragePartition>,
    raised: Receiver<u64>,
) -> Result<std::thread::JoinHandle<()>, RoleError> {
    #[expect(
        clippy::disallowed_methods,
        reason = "the store's synced write runs off the core on a real thread, which no simulation replays"
    )]
    let spawned = std::thread::Builder::new()
        .name(format!("discard-{}", id.get()))
        .spawn(move || {
            // Ends once every sender is gone, which is the partition stopping (Ultra rule 2).
            while let Ok(first) = raised.recv() {
                let latest = raised.try_iter().last().unwrap_or(first);
                let lists = partition.applier().quads().lists();
                if let Err(error) = lists.set_discard_ts(latest) {
                    tracing::error!(partition = id.get(), %error, "the discard version was not raised");
                }
            }
        });
    spawned.map_err(|error| RoleError::unavailable(format!("a partition's thread: {error}")))
}

/// What a recovered partition serves with.
struct Serving<'a> {
    host: StorageHost<'a>,
    remote: std::sync::mpsc::Receiver<RemoteMessage>,
    self_sender: std::sync::mpsc::Sender<PartitionMessage>,
    leased: LeasedBlocks,
    wants: UnboundedReceiver<TypePrefix>,
    delivered: Epoch,
    joined: oneshot::Sender<()>,
    discards: std::sync::mpsc::Sender<u64>,
    tenants: Vec<u64>,
}

/// Listens at the partition's address, leases its first blocks, joins the frontend and subscribes to its closes, then runs
/// the partition's core until the service stops or the frontend stays away past the deadline.
///
/// A future dropped at any await leaves the tasks it spawned, the accept loop, the answers' forwarding, the frontend's
/// requests and the closes' delivery, running until their runtime ends; one dropped while the core runs drops the core
/// and its host.
async fn serve_partition(
    start: &CoreStart,
    serving: Serving<'_>,
    stopping: &Stopping,
) -> Result<(), RoleError> {
    let own = start.id;
    let cluster = &start.cluster;
    let name = RoleKind::Storage.to_string();
    let address =
        partition_address(&start.base, start.first, own.get()).map_err(RoleError::configuration)?;
    // Each connection's requests within its credit and its `held_bytes` of frames parked, its few messages of opening,
    // ending and cancelling, and the catalog versions a close announces.
    let (to_core, inbox) = unbounded();
    accept_connections(listen(&address).await?, start, to_core.clone())?;
    // Each ask comes from an epoch or a catalog event, not from a request: a close request per epoch the commit step
    // holds on, a default entry per declaration taking effect, a report per catalog version.
    let (asks_tx, asks) = unbounded();
    // A close per epoch, which the core drains each turn.
    let (closes_tx, closes) = std::sync::mpsc::channel();
    // One answer per registration this partition asked of another process, forwarded as it arrives.
    let (answers_tx, answers) = unbounded();
    let peers = Peers::new(
        cluster.clone(),
        start.frontend.clone(),
        name.clone(),
        answers_tx,
    );
    spawn(forward_answers(answers, serving.self_sender.clone())).detach();
    for prefix in [TypePrefix::RESOURCE, TypePrefix::STATEMENT] {
        top_up(&peers, &serving.leased, own, prefix, stopping).await?;
    }
    spawn(frontend_requests(
        peers.clone(),
        serving.leased,
        own,
        serving.wants,
        asks,
    ))
    .detach();
    let join = Join::storage(address, vec![own.get()]);
    let Some(subscription) = retry_until(cluster, &name, stopping, || {
        joined_subscription(&peers, &join, serving.delivered)
    })
    .await?
    else {
        return Ok(());
    };
    // A role gone before its core was ready has nothing left to run for.
    let _told: Result<(), ()> = serving.joined.send(());
    let (lost_tx, lost) = oneshot::channel();
    spawn(deliver_closes(
        subscription,
        peers.clone(),
        join,
        serving.delivered,
        (closes_tx, to_core),
        lost_tx,
        stopping.clone(),
    ))
    .detach();
    let channels = CoreChannels::new(
        inbox,
        serving.remote,
        serving.self_sender,
        closes,
        asks_tx,
        serving.discards,
    );
    let core = PartitionCore::new(core_parts(
        start,
        serving.host,
        channels,
        peers,
        serving.tenants,
    ));
    until_lost(core.run(stopping), lost).await
}

/// What `running` stops with, or the failure `lost` tells of first.
///
/// A future dropped at its wait drops `running` with it.
async fn until_lost(
    running: impl Future<Output = Result<(), RoleError>>,
    mut lost: oneshot::Receiver<RoleError>,
) -> Result<(), RoleError> {
    let mut running = std::pin::pin!(running);
    std::future::poll_fn(|cx| {
        if let std::task::Poll::Ready(Ok(failure)) = std::pin::Pin::new(&mut lost).poll(cx) {
            return std::task::Poll::Ready(Err(failure));
        }
        running.as_mut().poll(cx)
    })
    .await
}

/// What the partition's core is made of, as the settings say.
fn core_parts<'a>(
    start: &CoreStart,
    host: StorageHost<'a>,
    channels: CoreChannels,
    peers: Peers,
    tenants: Vec<u64>,
) -> CoreParts<'a> {
    let table = start.config.storage();
    let retain = table
        .read_retain_ms()
        .unwrap_or(StorageTable::READ_RETAIN_MS_DEFAULT);
    CoreParts::new(
        host,
        start.id,
        start.cluster.id(),
        now(),
        channels,
        peers,
        Retention::new(retain),
    )
    .with_scan_stall_ms(table.scan_stall_ms().unwrap_or(SCAN_STALL_MS_DEFAULT))
    .with_tenants(tenants)
}

/// The latest close certainly delivered before the version `applied`: a commit is stamped one above the close before it.
fn last_close(applied: u64) -> Epoch {
    let epoch = Version::from_u64(applied).epoch().get();
    Epoch::new(epoch.saturating_sub(1)).unwrap_or(Epoch::ZERO)
}

fn close_request(config: &ServiceConfig) -> Result<CloseRequest, RoleError> {
    let Some(ms) = config.storage().close_request_ms() else {
        return Ok(CloseRequest::default());
    };
    let epoch_ms_max = config.sequencer().epoch_ms_max().unwrap_or(ms / 2);
    CloseRequest::new(ms, epoch_ms_max).map_err(|error| RoleError::configuration(error.to_string()))
}

fn open_partition(
    dir: &Path,
    id: PartitionId,
    placement: &PlacementMap,
    blocks: RefillBlocks,
) -> Result<StoragePartition, RoleError> {
    // A closure takes no type parameter, so it takes each error as a trait object (Ultra rules 1 and 9).
    let failed = |what: &str, why: &dyn std::fmt::Display| RoleError::io(format!("{what}: {why}"));
    let owned =
        Partition::<FjallStore, MokaListCache<CachedList>, ThreadExecutor, RefillBlocks>::owned(
            id,
            placement,
            Epoch::ZERO,
        )
        .ok_or_else(|| RoleError::software("a placement map's ranges overlap"))?;
    std::fs::create_dir_all(dir).map_err(|error| failed("the store's directory", &error))?;
    // The log is the record of what committed, so the store need not sync its own batches.
    let store =
        FjallStore::open(dir, Durability::Log).map_err(|error| failed("the store", &error))?;
    let cache = MokaListCache::new(CACHE_BYTES, |_: &[u8], list: &CachedList| list.weight());
    let lists = Lists::new(store, cache, ListsConfig::default());
    let mut seed = [0u8; 8];
    getrandom::fill(&mut seed)
        .map_err(|error| RoleError::unavailable(format!("the random source: {error}")))?;
    let quads = QuadStore::open(lists, blocks, RandomSource::new(u64::from_le_bytes(seed)))
        .map_err(|error| failed("the quads", &error))?
        .owning(owned);
    let applier = ChangesetApplier::over(quads, ThreadExecutor::new(), 1)
        .map_err(|error| failed("the applier", &error))?;
    Ok(Partition::new(id, placement.clone(), applier))
}

fn accept_connections(
    listener: Listener,
    start: &CoreStart,
    core: UnboundedSender<CoreMessage>,
) -> Result<(), RoleError> {
    let credentials: RoleCredentials = start.cluster.secret().credentials()?;
    let held = start.config.storage().held_bytes();
    let held_bytes = held.unwrap_or(StorageTable::HELD_BYTES_DEFAULT);
    let held_bytes = usize::try_from(held_bytes).unwrap_or(usize::MAX);
    let server = Server::new(start.cluster.id(), "storage", Endpoint::Partition)?
        .with_welcome(|welcome| welcome.with_initial_credit(PARTITION_INITIAL_CREDIT))
        .with_iterations(ROLE_ITERATIONS);
    spawn(async move {
        let mut connection = 0u64;
        let mut pace = AcceptPace::default();
        // An event loop: it accepts for as long as its runtime runs, which ends with the service (Ultra rule 2).
        loop {
            let carrier = pace.accept("role", || listener.accept()).await;
            connection += 1;
            let session = StorageSession::new(connection, core.clone(), held_bytes);
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

/// Every catalog version of `tenants` the frontend holds with an effective epoch, for a replayed record naming one. A tenant's
/// versions end at the first it never held, which the group refuses with code 7; any other failure fails the whole read, so a
/// connection lost part way gives no partial set.
///
/// A future dropped at a dial or a request drops the connection and the versions read so far: the read is whole or
/// nothing.
async fn held_versions(
    frontend: &Address,
    cluster: &RoleCluster,
    name: &str,
    tenants: &[u64],
) -> Result<HeldCatalogs, TransportError> {
    let mut held = HeldCatalogs::default();
    let mut client =
        RoleClient::dial(frontend, cluster.id(), name, cluster.secret().password()).await?;
    for tenant in tenants {
        let mut version = 1;
        loop {
            let reply = match client.request(&Catalog::request(*tenant, version)).await {
                Ok(reply) => reply,
                Err(TransportError(TransportErrorEnum::Refused {
                    code: ErrorCode::Refused,
                    ..
                })) => break,
                Err(error) => return Err(error),
            };
            let view = reply.view().map_err(TransportError::protocol)?;
            let answer = CatalogView::from_frame(&view).map_err(TransportError::protocol)?;
            if answer.epoch() != 0 {
                held.hold(HeldVersion::new(
                    *tenant,
                    version,
                    answer.epoch(),
                    answer.bytes().to_vec(),
                ));
            }
            drop(reply);
            version += 1;
        }
    }
    Ok(held)
}

/// The catalog version `version` announces, read from the frontend: what its close's delivery waits for.
///
/// A future dropped at a dial or the request drops the connection; a read changes nothing on the frontend.
async fn announced_catalog(
    peers: &Peers,
    version: &CatalogVersion,
) -> Result<CatalogContents, TransportError> {
    let mut client = RoleClient::dial(
        peers.frontend(),
        peers.cluster().id(),
        peers.name(),
        peers.cluster().secret().password(),
    )
    .await?;
    let tenant = version.tenant().get();
    let reply = client
        .request(&Catalog::request(tenant, version.version()))
        .await?;
    let view = reply.view().map_err(TransportError::protocol)?;
    let answer = CatalogView::from_frame(&view).map_err(TransportError::protocol)?;
    CatalogContents::decode(tenant, version.version(), answer.bytes())
        .map_err(|error| TransportError::authentication(error.to_string()))
}

/// The map the frontend holds, checked against the even map this stage holds fixed, and the tenants its runs list.
///
/// A future dropped at a dial or the request drops the connection; a read changes nothing on the frontend.
async fn placement_of(
    frontend: &Address,
    cluster: &RoleCluster,
    name: &str,
) -> Result<(PlacementMap, Vec<u64>), TransportError> {
    let mut client =
        RoleClient::dial(frontend, cluster.id(), name, cluster.secret().password()).await?;
    let reply = client.request(&Placement::request(0)).await?;
    let view = reply.view().map_err(TransportError::protocol)?;
    let placed = PlacementView::from_frame(&view).map_err(TransportError::protocol)?;
    let partitions = (0..placed.range_count())
        .filter_map(|index| placed.range(index))
        .map(|range| range.partition())
        .max()
        .ok_or_else(|| TransportError::authentication("a placement of no range"))?;
    let map = PlacementMap::even(partitions, NodeId::new(1))
        .map_err(|error| TransportError::authentication(error.to_string()))?;
    let held = map.ranges_at(Epoch::ZERO);
    let same = held.len() == placed.range_count()
        && held.iter().enumerate().all(|(index, (range, partition))| {
            placed.range(index).is_some_and(|wire| {
                wire.start() == range.start()
                    && wire.end() == range.end()
                    && wire.partition() == partition.get()
            })
        });
    if !same {
        return Err(TransportError::authentication(
            "the frontend's placement is not the even map this stage holds",
        ));
    }
    let tenants = (0..placed.run_count())
        .filter_map(|index| placed.run(index))
        .map(|run| run.tenant())
        .collect();
    Ok((map, tenants))
}

/// Joins for this partition and subscribes from `from`: the connection the closes arrive on.
///
/// A future dropped past the Join leaves the frontend holding this partition joined, which the next attempt joins
/// again; the subscription is dropped with its connection.
async fn joined_subscription(
    peers: &Peers,
    join: &Join,
    from: Epoch,
) -> Result<RoleClient, TransportError> {
    let mut client = RoleClient::dial(
        peers.frontend(),
        peers.cluster().id(),
        peers.name(),
        peers.cluster().secret().password(),
    )
    .await?;
    drop(client.request(join).await?);
    client.send_only(&Subscribe::new(from.get())).await?;
    Ok(client)
}

/// Hands each close the subscription brings to the partition's mailbox, joining and subscribing again when the frontend
/// goes away, from the last close it holds, until the deadline; past it, `lost` ends the core.
///
/// A future dropped at a read, a catalog version's fetch or a rejoin leaves the closes handed on so far delivered and
/// the one under way undelivered, its catalog versions dropped; `lost` dropped with it ends nothing.
async fn deliver_closes(
    mut client: RoleClient,
    peers: Peers,
    join: Join,
    mut after: Epoch,
    (closes, to_core): (
        std::sync::mpsc::Sender<PartitionMessage>,
        UnboundedSender<CoreMessage>,
    ),
    lost: oneshot::Sender<RoleError>,
    stopping: Stopping,
) {
    while !stopping.is_stopping() {
        let read = client.next_frame().await;
        // Asking again finds the same: the frontend no longer holds the close this partition needs (`PROTOCOL.md` §13.3).
        if let Some(ended) = read.as_ref().ok().and_then(subscription_end) {
            drop(lost.send(RoleError::unavailable(format!(
                "the frontend ended this process's subscription: {ended}"
            ))));
            return;
        }
        let close = read.and_then(|frame| close_of(&frame));
        match close {
            Ok(close) => {
                if close.epoch().get() != after.get() + 1 {
                    // A core gone has already stopped.
                    drop(lost.send(RoleError::software(format!(
                        "the frontend delivered close {} after close {}: a close follows the last by one epoch",
                        close.epoch().get(),
                        after.get()
                    ))));
                    return;
                }
                // A close is delivered once the process holds every catalog version it announces (`PROTOCOL.md` §11.12).
                let mut fetched = Vec::new();
                for version in close.catalog() {
                    match announced_catalog(&peers, version).await {
                        Ok(contents) => fetched.push(contents),
                        Err(error) => {
                            drop(lost.send(RoleError::unavailable(format!(
                                "a catalog version a close announces: {error}"
                            ))));
                            return;
                        }
                    }
                }
                for contents in fetched {
                    // A core gone holds nothing to install.
                    drop(to_core.unbounded_send(CoreMessage::Catalog(contents)));
                }
                after = close.epoch();
                // A core gone holds nothing to deliver to.
                drop(closes.send(PartitionMessage::Close(close)));
            }
            Err(error) => {
                tracing::warn!(%error, "the frontend's connection ended; joining again");
                let name = peers.name().to_string();
                let cluster = peers.cluster().clone();
                let again = retry_until(&cluster, &name, &stopping, || {
                    joined_subscription(&peers, &join, after)
                })
                .await;
                match again {
                    Ok(Some(next)) => client = next,
                    Ok(None) => return,
                    Err(failure) => {
                        // A core gone has already stopped.
                        drop(lost.send(failure));
                        return;
                    }
                }
            }
        }
    }
}

/// What the frontend said, when `frame` is the Error 1 ending the subscription; a frontend refusing as it stops is joined again.
fn subscription_end(frame: &ultragraphdb_protocol_transport::ReceivedFrame) -> Option<String> {
    let view = frame.view().ok()?;
    (view.command() == Command::Error).then_some(())?;
    let refused = ErrorReplyView::from_frame(&view).ok()?;
    (refused.code() == ErrorCode::Protocol).then(|| refused.message().to_string())
}

/// The close `frame` carries. A close always fits one frame (`PROTOCOL.md` §13.4 allows more, this frontend sends one).
fn close_of(
    frame: &ultragraphdb_protocol_transport::ReceivedFrame,
) -> Result<Close, TransportError> {
    let view = frame.view().map_err(TransportError::protocol)?;
    let close = EpochCloseView::from_frame(&view).map_err(TransportError::protocol)?;
    let wrong = |what: String| TransportError::authentication(what);
    let epoch = Epoch::new(close.epoch()).map_err(|error| wrong(error.to_string()))?;
    let catalog: Vec<CatalogVersion> = (0..close.announcement_count())
        .filter_map(|index| close.announcement(index))
        .map(|announced| CatalogVersion::new(Tenant::new(announced.tenant()), announced.version()))
        .collect();
    let evictions: Vec<u128> = (0..close.eviction_count())
        .filter_map(|index| close.eviction(index))
        .map(u128::from_be_bytes)
        .collect();
    Ok(Close::new(epoch, close.time(), catalog, evictions)
        .map_err(|error| wrong(error.to_string()))?
        .with_placement(close.placement()))
}

/// Forwards what a registration's task learned to the partition's mailbox, whose senders are not on this runtime's type.
///
/// A future dropped at its wait drops the answers still queued; each asker sends its registration again a second later.
async fn forward_answers(
    mut answers: UnboundedReceiver<PartitionMessage>,
    own: std::sync::mpsc::Sender<PartitionMessage>,
) {
    // Ends once every sender of `answers` is gone (Ultra rule 2).
    while let Some(answer) = answers.next().await {
        // A mailbox gone is a partition stopping.
        drop(own.send(answer));
    }
}

/// Leases until `prefix` is held ahead of need, retrying until the deadline.
///
/// A future dropped at a dial or a lease leaves the blocks supplied so far held; a block the frontend granted whose
/// reply was not read goes unused, its sequences skipped.
async fn top_up(
    peers: &Peers,
    leased: &LeasedBlocks,
    own: PartitionId,
    prefix: TypePrefix,
    stopping: &Stopping,
) -> Result<(), RoleError> {
    let name = peers.name().to_string();
    let _done = retry_until(peers.cluster(), &name, stopping, || async {
        let mut client = RoleClient::dial(
            peers.frontend(),
            peers.cluster().id(),
            peers.name(),
            peers.cluster().secret().password(),
        )
        .await?;
        lease_ahead(&mut client, leased, own, prefix).await
    })
    .await?;
    Ok(())
}

/// Leases blocks of `prefix` on `client` until [`BLOCKS_AHEAD`] are held. A future dropped at a lease leaves the blocks
/// supplied so far held; a block the frontend granted whose reply was not read goes unused, its sequences skipped.
async fn lease_ahead(
    client: &mut RoleClient,
    leased: &LeasedBlocks,
    own: PartitionId,
    prefix: TypePrefix,
) -> Result<(), TransportError> {
    while leased.held(prefix) < BLOCKS_AHEAD {
        let reply = client
            .request(&Lease::request(prefix.get(), own.get(), BLOCK_SIZE))
            .await?;
        let view = reply.view().map_err(TransportError::protocol)?;
        let block = LeaseView::from_frame(&view).map_err(TransportError::protocol)?;
        let sequence = SequenceBlock::new(block.start(), block.end())
            .ok_or_else(|| TransportError::authentication("an empty block"))?;
        leased
            .supply(prefix, sequence)
            .map_err(|error| TransportError::authentication(error.to_string()))?;
    }
    Ok(())
}

/// What the core asks of the frontend, in turn on one connection: leases ahead of need, and closes it asks for.
///
/// A future dropped at its wait or a request drops the connection and what it was asking, which the core asks again: a
/// lease at its next allocation, a close request as the commit step holds on, a default entry by the next recovery.
async fn frontend_requests(
    peers: Peers,
    leased: LeasedBlocks,
    own: PartitionId,
    mut wants: UnboundedReceiver<TypePrefix>,
    mut asks: UnboundedReceiver<FrontendAsk>,
) {
    let mut client: Option<RoleClient> = None;
    // An event loop: either channel's end, which is the core stopping, ends it (Ultra rule 2).
    loop {
        let next = std::future::poll_fn(|cx| {
            if let std::task::Poll::Ready(want) = wants.poll_next_unpin(cx) {
                return std::task::Poll::Ready(want.map(Asked::Lease));
            }
            asks.poll_next_unpin(cx).map(|ask| ask.map(Asked::Frontend))
        })
        .await;
        let Some(asked) = next else {
            return;
        };
        let asked = match asked {
            Asked::Frontend(FrontendAsk::Report(report)) => {
                spawn(send_report(peers.clone(), report)).detach();
                continue;
            }
            Asked::Lease(_)
            | Asked::Frontend(FrontendAsk::Close(_) | FrontendAsk::DefaultEntry(_)) => asked,
        };
        let held = match client.take() {
            Some(open) => Ok(open),
            None => {
                RoleClient::dial(
                    peers.frontend(),
                    peers.cluster().id(),
                    peers.name(),
                    peers.cluster().secret().password(),
                )
                .await
            }
        };
        let done = match held {
            Ok(mut open) => {
                let result = match asked {
                    Asked::Lease(prefix) => lease_ahead(&mut open, &leased, own, prefix).await,
                    Asked::Frontend(FrontendAsk::Close(epoch)) => {
                        open.send_only(&CloseRequestBody::new(own.get(), epoch.get()))
                            .await
                    }
                    Asked::Frontend(FrontendAsk::DefaultEntry(entry)) => {
                        let record =
                            DefaultEntry::request(entry.tenant(), entry.predicate().to_string());
                        open.request(&record).await.map(drop)
                    }
                    // Taken above, each on a connection of its own.
                    Asked::Frontend(FrontendAsk::Report(_)) => Ok(()),
                };
                client = result.is_ok().then_some(open);
                result
            }
            Err(error) => Err(error),
        };
        if let Err(error) = done {
            // A lease is asked again as the next block is taken, a close request again as the commit step holds on, and a
            // default entry again by the next recovery (`PROTOCOL.md` §11.12).
            tracing::warn!(%error, "a request to the frontend failed");
        }
    }
}

/// Sends `report` until the frontend answers it, on a connection of its own so a wait for the frontend holds no lease, or until the
/// role's deadline, past which the frontend's own wait for it has ended (`PROTOCOL.md` §13.19).
///
/// A future dropped at a dial, the request or a wait drops the report, which the close delivered again checks again.
async fn send_report(peers: Peers, report: Report) {
    let due = now() + peers.cluster().deadline();
    loop {
        let dialled = RoleClient::dial(
            peers.frontend(),
            peers.cluster().id(),
            peers.name(),
            peers.cluster().secret().password(),
        )
        .await;
        let answered = match dialled {
            Ok(mut client) => client.request(&report).await.map(drop),
            Err(error) => Err(error),
        };
        match answered {
            Ok(()) => return,
            Err(error) if now() >= due => {
                tracing::warn!(%error, version = report.version(), "a catalog version's report went unanswered");
                return;
            }
            Err(_) => compio::time::sleep(REPORT_AGAIN).await,
        }
    }
}

/// What the core's frontend connection takes next.
enum Asked {
    Lease(TypePrefix),
    Frontend(FrontendAsk),
}
