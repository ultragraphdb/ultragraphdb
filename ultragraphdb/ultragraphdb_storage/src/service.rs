/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use compio::runtime::{JoinHandle, spawn};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures::channel::oneshot;
use ultragraphdb_cluster_metadata::{LeasedBlocks, MetadataConfig};
use ultragraphdb_cluster_partition::{
    Partition, PartitionHost, PartitionMailbox, PartitionMessage, RemoteMessage,
};
use ultragraphdb_cluster_placement::{NodeId, PartitionId, PlacementMap};
use ultragraphdb_cluster_sequencer::{CatalogVersion, Close, SequencerConfig};
use ultragraphdb_common_role::{
    Answered, PARTITION_INITIAL_CREDIT, ROLE_ITERATIONS, RoleClient, RoleCluster, RoleCredentials,
    Server, listen, retry_until, serve_connection, system_address,
};
use ultragraphdb_common_service::{Readiness, RoleKind, RoleStart, Stopping, core_runtime};
use ultragraphdb_common_service_config::{
    ServiceConfig, StorageTable, SystemTable, partition_address,
};
use ultragraphdb_common_storage::{
    CoreChannels, CoreEnds, CoreMessage, CoreParts, HeldCatalogs, LogIo, PartitionCore, Peers,
    RefillBlocks, Retention, RoleConnectionPhases, SCAN_STALL_MS_DEFAULT, StorageHost,
    StoragePartition, StorageSession, SystemAsk, TimedStorage,
};
use ultragraphdb_common_system::{SystemSession, SystemShared, SystemStart, start_system};
use ultragraphdb_core_catalog::CatalogContents;
use ultragraphdb_core_ids::{Epoch, Tenant, TypePrefix, Version};
use ultragraphdb_core_mapping::RandomSource;
use ultragraphdb_error_role::{RoleError, RoleErrorEnum};
use ultragraphdb_protocol_transport::{Address, Endpoint, TransportError, TransportErrorEnum};
use ultragraphdb_protocol_transport_compio::{AcceptPace, Listener};
use ultragraphdb_protocol_wire::{
    BodyView, Catalog, CatalogFrames, CloseRequest as CloseRequestBody, Command, Credit,
    DefaultEntry, EpochCloseView, ErrorCode, FLAG_LAST, FrameView, Join, Lease, LeaseView,
    Placement, PlacementView, Report, Subscribe,
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

/// The frames a catalog version takes at most: its bytes, at most the wire's `CATALOG_BYTES_MAX` of 16 MiB, fill each frame
/// to the 1 MiB `max_frame` a role's Hello announces (`PROTOCOL.md` §11.12, §13.7), so 17 hold the largest.
const CATALOG_FRAMES_MAX: usize = 64;

/// The catalog versions of one tenant a recovery reads at most, each a declaring transaction's or a Drop's: past it the
/// read is refused, rather than looping on a group that never refuses (Ultra rule 2).
const VERSIONS_PER_TENANT_MAX: u64 = 1 << 20;

/// How many looks a stop gives the system group to end once its connections are closed, one each [`POLL`]: 5 s.
const GROUP_STOP_LOOKS: u32 = 250;

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
        // Microseconds since 1970 fit a `u64` until the year 586,912; a clock past that saturates.
        .map_or(1, |since| {
            u64::try_from(since.as_micros()).unwrap_or(u64::MAX)
        })
}

/// What every partition core of the process reads at start.
#[derive(Debug, Clone)]
struct CoreStart {
    config: ServiceConfig,
    cluster: RoleCluster,
    system: Address,
    base: String,
    first: u32,
    data_dir: PathBuf,
    core: usize,
    id: PartitionId,
}

/// The storage role: a thread and a runtime per partition, each recovering, listening and joining for itself, and in the
/// instance whose settings hold `[system]` the system group on a thread and runtime of its own, recovered and listening
/// before any partition starts (design D3, D10). It is ready once its partitions are, and at a stop each partition flushes
/// what its log holds through its store, and then the group stops.
///
/// # Errors
///
/// [`RoleError::configuration`] for a setting absent or out of its bounds or a refused role secret;
/// [`RoleError::unavailable`] when a thread cannot start or the system group does not take the role by its deadline, naming
/// its address; [`RoleError::io`] for a failed store or log, the group's among them.
///
/// A future dropped at any await leaves the partitions' threads and the group's running, each ending at the service's stop,
/// which no one then waits for.
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
    // The group's address the partitions dial (design D2, D4), a hosting instance's partitions included.
    let system = system_address(&start)?;
    let ids = partition_ids(first, count)?;
    // The group first: a partition's recovery reads the catalog versions its log names from it (design D10).
    let mut hosted = match config.system() {
        Some(system_table) => Some(Hosted::start(&start, &cluster, system_table, ids.len()).await?),
        None => None,
    };
    // What every core starts from; `spawn_cores` gives each its core and its partition.
    let core_start = CoreStart {
        config,
        cluster,
        system,
        base,
        first,
        data_dir,
        core: 0,
        id: *ids
            .first()
            .ok_or_else(|| RoleError::software("a partition at least"))?,
    };
    let (mut ended, ready) = spawn_cores(&core_start, &ids, &stopping)?;
    for is_ready in ready {
        match wait_or_end(is_ready, &mut ended, hosted.as_mut(), &stopping).await {
            Ok(true) => {}
            Ok(false) => return stop_in_order(ended, hosted).await,
            Err(failure) => return stop_the_rest(failure, ended, hosted, &stopping).await,
        }
    }
    readiness.set(true);
    tracing::info!(partitions = count, "ready");
    // An event loop: the service's stop, a core's end or the group's end ends it (Ultra rule 2).
    while !stopping.is_stopping() {
        if let Err(failure) = look(&mut ended, hosted.as_mut()) {
            return stop_the_rest(failure, ended, hosted, &stopping).await;
        }
        compio::time::sleep(POLL).await;
    }
    stop_in_order(ended, hosted).await
}

/// The partitions `storage.partition_first` and `storage.partition_count` name.
fn partition_ids(first: u32, count: u32) -> Result<Vec<PartitionId>, RoleError> {
    (0..count)
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
        .collect()
}

/// Starts a core per partition of `ids`, the `at`th on core `at`, each as `start` says: how each ends, and when each is
/// ready.
fn spawn_cores(
    start: &CoreStart,
    ids: &[PartitionId],
    stopping: &Stopping,
) -> Result<(CoreEnds, Vec<oneshot::Receiver<()>>), RoleError> {
    let mut mailboxes = PartitionMailbox::mesh(ids);
    let mut ended = CoreEnds::default();
    let mut ready = Vec::new();
    for (core, id) in ids.iter().enumerate().rev() {
        let mailbox = mailboxes
            .pop()
            .ok_or_else(|| RoleError::software("a mailbox per partition"))?;
        let core_start = CoreStart {
            core,
            id: *id,
            ..start.clone()
        };
        let (joined, is_ready) = oneshot::channel();
        ready.push(is_ready);
        ended.add(spawn_core(core_start, mailbox, joined, stopping.clone())?);
    }
    Ok((ended, ready))
}

/// Whether a partition ended cleanly since the last look; a partition's failure, or the hosted group's end, which is always
/// a failure while the role runs.
fn look(ended: &mut CoreEnds, hosted: Option<&mut Hosted>) -> Result<bool, RoleError> {
    if let Some(group) = hosted {
        group.look()?;
    }
    ended.look()
}

/// The ordinary stop: every partition flushes what its log holds, and then the hosted group stops (design D10).
///
/// A future dropped while it waits leaves the partitions flushing on their threads and the group running, unwaited.
async fn stop_in_order(ended: CoreEnds, hosted: Option<Hosted>) -> Result<(), RoleError> {
    let flushed = ended.finish().await;
    let stopped = match hosted {
        Some(group) => group.stop().await,
        None => Ok(()),
    };
    flushed.and(stopped)
}

/// A partition or the hosted group failed, so the process shuts down through its ordinary path: every other partition flushes
/// what its log holds, the group stops after them, and the process exits with the failure's code.
///
/// A future dropped while it waits leaves the stop set and the partitions flushing on their threads, unwaited.
async fn stop_the_rest(
    failure: RoleError,
    ended: CoreEnds,
    hosted: Option<Hosted>,
    stopping: &Stopping,
) -> Result<(), RoleError> {
    tracing::error!(%failure, "a partition or the system group stopped; stopping the rest");
    stopping.set();
    // The first failure is the one reported.
    drop(stop_in_order(ended, hosted).await);
    tracing::info!(role = %RoleKind::Storage, "stopping");
    Err(failure)
}

/// Whether the core behind `is_ready` became ready; false once the service stops first. A core ending first ends this, and
/// so does the hosted group's end.
///
/// A future dropped at its wait holds nothing: a core then telling its readiness finds no one, which it takes as a role
/// gone.
async fn wait_or_end(
    mut is_ready: oneshot::Receiver<()>,
    ended: &mut CoreEnds,
    mut hosted: Option<&mut Hosted>,
    stopping: &Stopping,
) -> Result<bool, RoleError> {
    // An event loop: readiness, a core's end, the group's end or the service's stop ends it (Ultra rule 2).
    loop {
        if matches!(is_ready.try_recv(), Ok(Some(()))) {
            return Ok(true);
        }
        if look(ended, hosted.as_deref_mut())? || stopping.is_stopping() {
            return Ok(false);
        }
        compio::time::sleep(POLL).await;
    }
}

/// The system group this process hosts: what its thread stops with, and the task accepting its role connections, which holds
/// each connection's task, so dropping it closes every connection and lets go of every way to the group.
struct Hosted {
    stopped: Option<oneshot::Receiver<Result<(), RoleError>>>,
    accepting: JoinHandle<()>,
}

impl Hosted {
    /// Starts the group as `table` says, on core `core`, the one after the partitions', and listens at its role address once
    /// it has recovered: the group's quiet time begins as it listens (`PROTOCOL.md` §11.5).
    ///
    /// A future dropped at the start leaves what [`start_system`] leaves; one dropped at the listen, the group running until
    /// its handle, dropped with it, is gone.
    async fn start(
        start: &RoleStart,
        cluster: &RoleCluster,
        table: &SystemTable,
        core: usize,
    ) -> Result<Self, RoleError> {
        let config = start.config();
        let address = table.role_address();
        // Parsed here so a malformed one is refused naming its setting; `listen` reads the text again.
        let _parsed: Address = Address::parse(address).map_err(|error| {
            RoleError::configuration(format!(
                "the setting system.role_address holds {address}: {error}"
            ))
        })?;
        let begun = system_start(config)?.with_core(core);
        let data_dir = PathBuf::from(table.data_dir());
        let group = start_system(config.clone(), cluster.id(), data_dir, begun, Vec::new()).await?;
        let (handle, stopped) = group.into_parts();
        let server = Server::new(cluster.id(), "storage", Endpoint::Frontend)?
            .with_iterations(ROLE_ITERATIONS);
        let shared = SystemShared::new(cluster.clone(), handle);
        let listener = listen(address).await?;
        let credentials = cluster.secret().credentials()?;
        let accepting = spawn(accept_system(listener, server, credentials, shared));
        tracing::info!(address, "the system group listens");
        Ok(Self {
            stopped: Some(stopped),
            accepting,
        })
    }

    /// Nothing while the group runs; what it stopped with otherwise, which is a failure while the role runs.
    fn look(&mut self) -> Result<(), RoleError> {
        let Some(stopped) = self.stopped.as_mut() else {
            return Ok(());
        };
        let outcome = match stopped.try_recv() {
            Ok(None) => return Ok(()),
            Ok(Some(Err(failure))) => failure,
            Ok(Some(Ok(()))) => RoleError::software("the system group stopped while the role runs"),
            Err(_) => RoleError::software("the system group's thread ended without a word"),
        };
        self.stopped = None;
        Err(outcome)
    }

    /// Closes the group's connections, which lets go of every way to it, and waits for it to stop, its last turn's sync done,
    /// within [`GROUP_STOP_LOOKS`].
    ///
    /// A future dropped at its wait leaves the group stopping on its thread, unheard.
    async fn stop(self) -> Result<(), RoleError> {
        let Self { stopped, accepting } = self;
        drop(accepting);
        let Some(mut stopped) = stopped else {
            return Ok(());
        };
        for _ in 0..GROUP_STOP_LOOKS {
            match stopped.try_recv() {
                Ok(None) => compio::time::sleep(POLL).await,
                Ok(Some(outcome)) => return outcome,
                Err(_) => {
                    return Err(RoleError::software(
                        "the system group's thread ended without a word",
                    ));
                }
            }
        }
        Err(RoleError::unavailable(
            "the system group did not stop within its limit once its connections closed",
        ))
    }
}

/// Accepts the roles' connections to the system group, each served in a task this task holds: one finished is let go at
/// the next accept, so it holds at most the connections open, and dropping this task closes them all.
///
/// A future dropped at an accept takes no connection half-way, and drops every connection's task.
async fn accept_system(
    listener: Listener,
    server: Server,
    credentials: RoleCredentials,
    shared: SystemShared,
) {
    let mut pace = AcceptPace::default();
    let mut connections: Vec<JoinHandle<()>> = Vec::new();
    // An event loop: it accepts until the stop drops it, or its runtime ends with the service (Ultra rule 2).
    loop {
        let carrier = pace.accept("role", || listener.accept()).await;
        connections.retain(|connection| !connection.is_finished());
        let (server, credentials) = (server.clone(), credentials.clone());
        let session = SystemSession::new(shared.clone());
        connections.push(spawn(async move {
            serve_connection(carrier, &server, credentials, session).await;
        }));
    }
}

/// What the settings say the system group is, each bound the state machines' own: `[metadata]`, `[sequencer]` and
/// `cluster.partition_count`, which a first start forms the placement over.
fn system_start(config: &ServiceConfig) -> Result<SystemStart, RoleError> {
    // A closure takes no type parameter, so it takes each error as a trait object (Ultra rules 1 and 9).
    let refused = |error: &dyn std::fmt::Display| RoleError::configuration(error.to_string());
    let partition_count = config
        .cluster()
        .partition_count()
        .filter(|count| *count > 0)
        .ok_or_else(|| {
            RoleError::configuration("the setting cluster.partition_count is at least 1")
        })?;
    let sequencer_table = config.sequencer();
    let required_ms = |value: Option<u64>, setting: &str| {
        value.ok_or_else(|| RoleError::configuration(format!("the setting {setting} is required")))
    };
    let sequencer = SequencerConfig::new(
        required_ms(sequencer_table.epoch_ms_min(), "sequencer.epoch_ms_min")?,
        required_ms(sequencer_table.epoch_ms_max(), "sequencer.epoch_ms_max")?,
    )
    .map_err(|error| refused(&error))?;
    let metadata = metadata_config(config)?;
    let begun = SystemStart::new(metadata, sequencer, partition_count);
    Ok(match config.metadata().bootstrap_secret_file() {
        Some(file) => begun.with_bootstrap_file(file),
        None => begun,
    })
}

/// The metadata group's settings, each bound its own.
fn metadata_config(config: &ServiceConfig) -> Result<MetadataConfig, RoleError> {
    // A closure takes no type parameter, so it takes each error as a trait object (Ultra rules 1 and 9).
    let refused = |error: &dyn std::fmt::Display| RoleError::configuration(error.to_string());
    let table = config.metadata();
    let mut metadata = MetadataConfig::new();
    if let Some(size) = table.block_size_max() {
        metadata = metadata
            .with_block_size_max(size)
            .map_err(|error| refused(&error))?;
    }
    if let Some(sessions) = table.sessions_max() {
        metadata = metadata
            .with_sessions_max(sessions)
            .map_err(|error| refused(&error))?;
    }
    if let Some(rounds) = table.snapshot_rounds() {
        metadata = metadata
            .with_snapshot_rounds(rounds)
            .map_err(|error| refused(&error))?;
    }
    if let Some(ms) = table.catalog_check_ms() {
        metadata = metadata
            .with_catalog_check_ms(ms)
            .map_err(|error| refused(&error))?;
    }
    Ok(metadata)
}

/// `failure` naming `address`, the system group's it waited on, when the group was not there by the deadline
/// (system-group, "A lost system group is waited for until the deadline"); any other failure as it is.
fn naming(address: &Address, failure: RoleError) -> RoleError {
    match failure.0 {
        RoleErrorEnum::Unavailable { message } => {
            RoleError::unavailable(format!("the system group at {address}: {message}"))
        }
        other @ (RoleErrorEnum::Configuration { .. }
        | RoleErrorEnum::Io { .. }
        | RoleErrorEnum::Software { .. }) => RoleError(other),
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
    // Detached: the thread's outcome comes through `ended`.
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
        placement_of(&start.system, cluster, &name)
    })
    .await
    .map_err(|failure| naming(&start.system, failure))?
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
    // group's task drains them as it runs, and nothing bounds them while it waits on the system group.
    let (wanted, wants) = unbounded();
    let blocks = RefillBlocks::new(leased.clone(), wanted);
    // Shared by the core and the discard thread, which raises the discard version off the core (Ultra rule 9).
    let partition = Arc::new(open_partition(&dir.join("store"), own, &placement, blocks)?);
    // At most one ask per core turn that raises the bound, each pass of the discard thread draining every ask: it holds
    // the turns of one synced write.
    let (discards, raised) = std::sync::mpsc::channel();
    let discarding = spawn_discards(own, Arc::clone(&partition), raised)?;
    let config = WalConfig::new(cluster.id(), false);
    let bounds = RoundBounds::new(config.round_bytes(), config.round_transactions());
    let (mut log, log_io) = open_log(own, &dir, config).await?;
    // A group that is not there yet, or stops under the start, is asked again until the join deadline, as joining is.
    let Some(held) = retry_until(cluster, &name, &stopping, || {
        held_versions(&start.system, cluster, &name, &tenants)
    })
    .await
    .map_err(|failure| naming(&start.system, failure))?
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
        log_io,
    };
    let outcome = serve_partition(&start, served, &stopping).await;
    // The core is gone, and its sender with it, so the thread ends past the write it may be in.
    discarding
        .join()
        .map_err(|_| RoleError::software("a partition's discard thread panicked"))?;
    outcome
}

/// The write-ahead log of partition `own` in `dir`, as `config` says, and where its writes and syncs are counted, for the core's
/// report of its commits' phases.
///
/// A future dropped at the open leaves what [`Wal::open`] leaves, which the next start opens again.
async fn open_log(
    own: PartitionId,
    dir: &Path,
    config: WalConfig,
) -> Result<(Wal<TimedStorage<CompioStorage>>, LogIo), RoleError> {
    let io = LogIo::default();
    let storage = TimedStorage::new(CompioStorage, io.clone(), now());
    let (log, _) = Wal::open(storage, dir.join("wal"), config)
        .await
        .map_err(|error| RoleError::io(format!("the partition {} log: {error}", own.get())))?;
    Ok((log, io))
}

/// `partition`'s catalog versions loaded and its log replayed over them: the version it applied through.
///
/// A future dropped at the replay leaves the store holding the rounds applied so far, which a recovery replays again
/// from the log.
async fn replayed(
    partition: &StoragePartition,
    log: &mut Wal<TimedStorage<CompioStorage>>,
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
    log_io: LogIo,
}

/// Listens at the partition's address, leases its first blocks, joins the system group and subscribes to its closes, then
/// runs the partition's core until the service stops or the group stays away past the deadline.
///
/// A future dropped at any await leaves the tasks it spawned, the accept loop, the answers' forwarding, the group's
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
    // Each connection's requests within its credit and its `held_bytes` of frames parked, its few messages of opening,
    // ending and cancelling, and the catalog versions a close announces.
    let (to_core, inbox) = unbounded();
    // What the connections' sessions time their requests into, which the core logs beside its commits' phases.
    let role_phases = RoleConnectionPhases::new(now());
    let address = accept_at(start, to_core.clone(), role_phases.clone()).await?;
    // Each ask comes from an epoch or a catalog event, not from a request: a close request per epoch the commit step
    // holds on, a default entry per declaration taking effect, a report per catalog version.
    let (asks_tx, asks) = unbounded();
    // A close per epoch, which the core drains each turn.
    let (closes_tx, closes) = std::sync::mpsc::channel();
    // One answer per registration this partition asked of another process, forwarded as it arrives.
    let (answers_tx, answers) = unbounded();
    let peers = Peers::new(
        cluster.clone(),
        start.system.clone(),
        name.clone(),
        answers_tx,
    );
    spawn(forward_answers(answers, serving.self_sender.clone())).detach();
    for prefix in [TypePrefix::RESOURCE, TypePrefix::STATEMENT] {
        top_up(&peers, &serving.leased, own, prefix, stopping).await?;
    }
    spawn(system_requests(
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
    .await
    .map_err(|failure| naming(&start.system, failure))?
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
    let core = PartitionCore::new(
        core_parts(start, serving.host, channels, peers, serving.tenants)
            .with_log_io(serving.log_io)
            .with_role_phases(role_phases),
    );
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
    // Half the wait, rounded down to whole milliseconds.
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

/// Listens at the partition's address, each connection served by a session handing its requests to `core` and timing them into
/// `phases`: the address.
///
/// A future dropped at the bind leaves nothing listening.
async fn accept_at(
    start: &CoreStart,
    core: UnboundedSender<CoreMessage>,
    phases: RoleConnectionPhases,
) -> Result<String, RoleError> {
    let address = partition_address(&start.base, start.first, start.id.get())
        .map_err(RoleError::configuration)?;
    accept_connections(listen(&address).await?, start, core, phases)?;
    Ok(address)
}

fn accept_connections(
    listener: Listener,
    start: &CoreStart,
    core: UnboundedSender<CoreMessage>,
    phases: RoleConnectionPhases,
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
            let session = StorageSession::new(connection, core.clone(), held_bytes)
                .with_phases(phases.clone());
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

/// Every catalog version of `tenants` the system group holds with an effective epoch, for a replayed record naming one, each
/// read across its Catalog frames. A tenant's versions end at the first it never held, which the group refuses with code 7;
/// any other failure fails the whole read, so a connection lost part way gives no partial set.
///
/// A future dropped at a dial or a request drops the connection and the versions read so far: the read is whole or
/// nothing.
async fn held_versions(
    system: &Address,
    cluster: &RoleCluster,
    name: &str,
    tenants: &[u64],
) -> Result<HeldCatalogs, TransportError> {
    let mut held = HeldCatalogs::default();
    let mut client =
        RoleClient::dial(system, cluster.id(), name, cluster.secret().password()).await?;
    for tenant in tenants {
        // Ends at the first version the group never held, or past `VERSIONS_PER_TENANT_MAX` (Ultra rule 2).
        for version in 1..=VERSIONS_PER_TENANT_MAX {
            let frames = match catalog_read(&mut client, *tenant, version).await {
                Ok(frames) => frames,
                Err(TransportError(TransportErrorEnum::Refused {
                    code: ErrorCode::Refused,
                    ..
                })) => break,
                Err(error) => return Err(error),
            };
            let answer = frames
                .view()
                .ok_or_else(|| TransportError::authentication("a Catalog answer of no frame"))?;
            tracing::info!(
                tenant,
                version,
                epoch = answer.epoch(),
                "a catalog version read"
            );
            if answer.epoch() != 0 {
                held.hold(HeldVersion::new(
                    *tenant,
                    version,
                    answer.epoch(),
                    answer.bytes().to_vec(),
                ));
            }
            if version == VERSIONS_PER_TENANT_MAX {
                return Err(TransportError::authentication(format!(
                    "tenant {tenant} holds more than {VERSIONS_PER_TENANT_MAX} catalog versions"
                )));
            }
        }
    }
    Ok(held)
}

/// Version `version` of `tenant` read on `client` across its Catalog frames, credit granted for each frame after the first
/// (`PROTOCOL.md` §13.7, design D13), at most [`CATALOG_FRAMES_MAX`] of them, each read by [`RoleClient::next_answer`]'s rule: a
/// late frame among them is checked whole and dropped (§2.5, step 3; §11.2).
///
/// # Errors
///
/// The group's Error on stream 0 or the version's stream, and a frame this process refuses (§2.6, §13): one breaking a
/// frame-level rule, one on stream 0 or a stream it has not opened, which answers no request, or a command a Catalog is not
/// answered by, each of which closes the connection as it is read; and one breaking its body's rule, for which the caller drops
/// the client, which closes the connection.
///
/// A future dropped at a send or a read leaves the connection part way through the answer: the caller drops it.
async fn catalog_read(
    client: &mut RoleClient,
    tenant: u64,
    version: u64,
) -> Result<CatalogFrames, TransportError> {
    let mut frames = CatalogFrames::new();
    let first = client.request(&Catalog::request(tenant, version)).await?;
    let stream = client.last_stream();
    let mut last = taken(
        &mut frames,
        &first.view().map_err(TransportError::protocol)?,
    )?;
    drop(first);
    // At most `CATALOG_FRAMES_MAX` frames, each read once its credit is granted (Ultra rule 2).
    for _ in 1..CATALOG_FRAMES_MAX {
        if last {
            return Ok(frames);
        }
        client.send_on(stream, &Credit::new(1)).await?;
        let next = client.next_answer(Command::Catalog).await?.frame()?;
        last = taken(&mut frames, &next.view().map_err(TransportError::protocol)?)?;
    }
    if last {
        return Ok(frames);
    }
    Err(TransportError::authentication(format!(
        "a catalog version of more than {CATALOG_FRAMES_MAX} frames"
    )))
}

/// Takes `view` into `frames`: whether it was the answer's last.
fn taken(frames: &mut CatalogFrames, view: &FrameView<'_>) -> Result<bool, TransportError> {
    frames.push(view).map_err(TransportError::protocol)?;
    Ok(view.header().flags() & FLAG_LAST != 0)
}

/// The catalog version `version` announces, read from the system group across its frames: what its close's delivery waits
/// for.
///
/// A future dropped at a dial or the request drops the connection; a read changes nothing in the group.
async fn announced_catalog(
    peers: &Peers,
    version: &CatalogVersion,
) -> Result<CatalogContents, TransportError> {
    let mut client = RoleClient::dial(
        peers.system(),
        peers.cluster().id(),
        peers.name(),
        peers.cluster().secret().password(),
    )
    .await?;
    let tenant = version.tenant().get();
    let frames = catalog_read(&mut client, tenant, version.version()).await?;
    let answer = frames
        .view()
        .ok_or_else(|| TransportError::authentication("a Catalog answer of no frame"))?;
    CatalogContents::decode(tenant, version.version(), answer.bytes())
        .map_err(|error| TransportError::authentication(error.to_string()))
}

/// The map the system group holds, checked against the even map this stage holds fixed, and the tenants its runs list.
///
/// A future dropped at a dial or the request drops the connection; a read changes nothing in the group.
async fn placement_of(
    system: &Address,
    cluster: &RoleCluster,
    name: &str,
) -> Result<(PlacementMap, Vec<u64>), TransportError> {
    let mut client =
        RoleClient::dial(system, cluster.id(), name, cluster.secret().password()).await?;
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
            "the system group's placement is not the even map this stage holds",
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
/// A future dropped past the Join leaves the group holding this partition joined, which the next attempt joins
/// again; the subscription is dropped with its connection.
async fn joined_subscription(
    peers: &Peers,
    join: &Join,
    from: Epoch,
) -> Result<RoleClient, TransportError> {
    let mut client = RoleClient::dial(
        peers.system(),
        peers.cluster().id(),
        peers.name(),
        peers.cluster().secret().password(),
    )
    .await?;
    drop(client.request(join).await?);
    client.send_only(&Subscribe::new(from.get())).await?;
    Ok(client)
}

/// Hands each close the subscription brings to the partition's mailbox, joining and subscribing again when the group
/// goes away, from the last close it holds, until the deadline; past it, `lost` ends the core. A connection whose frame this
/// process refuses is closed, as the side that dialled closes it (`PROTOCOL.md` §2.6, §13), and joined again on a new one.
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
        let read = client
            .next_answer(Command::Subscribe)
            .await
            .and_then(subscribed);
        let failure = match read {
            // Asking again finds the same: the group no longer holds the close this partition needs (`PROTOCOL.md` §13.3).
            Ok(Subscribed::Ended(ended)) => RoleError::unavailable(format!(
                "the system group ended this process's subscription: {ended}"
            )),
            Ok(Subscribed::Close(close)) => {
                match delivered(close, after, &peers, (&closes, &to_core)).await {
                    Ok(epoch) => {
                        after = epoch;
                        continue;
                    }
                    Err(failure) => failure,
                }
            }
            Err(error) => {
                tracing::warn!(%error, "the connection to the system group ended; joining again");
                // Closed before the next is dialled, so no connection outlives the one replacing it.
                drop(client);
                match rejoined(&peers, &join, after, &stopping).await {
                    Ok(Some(next)) => {
                        client = next;
                        continue;
                    }
                    Ok(None) => return,
                    Err(failure) => failure,
                }
            }
        };
        // A core gone has already stopped.
        drop(lost.send(failure));
        return;
    }
}

/// Joins for this partition and subscribes from `after` again, until the deadline: the new connection, or `None` once the
/// service stops.
///
/// A future dropped at a dial or a request drops the connection under way, as [`joined_subscription`] says.
async fn rejoined(
    peers: &Peers,
    join: &Join,
    after: Epoch,
    stopping: &Stopping,
) -> Result<Option<RoleClient>, RoleError> {
    let name = peers.name().to_string();
    let cluster = peers.cluster().clone();
    retry_until(&cluster, &name, stopping, || {
        joined_subscription(peers, join, after)
    })
    .await
    .map_err(|failure| naming(peers.system(), failure))
}

/// Delivers `close`, the one after close `after`, once the process holds every catalog version it announces (`PROTOCOL.md`
/// §11.12): its epoch.
///
/// # Errors
///
/// [`RoleError::software`] for a close not one epoch past `after`; [`RoleError::unavailable`] for a catalog version it
/// announces that could not be read.
///
/// A future dropped at a catalog version's fetch delivers nothing, the versions read so far dropped.
async fn delivered(
    close: Close,
    after: Epoch,
    peers: &Peers,
    (closes, to_core): (
        &std::sync::mpsc::Sender<PartitionMessage>,
        &UnboundedSender<CoreMessage>,
    ),
) -> Result<Epoch, RoleError> {
    if close.epoch().get() != after.get() + 1 {
        return Err(RoleError::software(format!(
            "the system group delivered close {} after close {}: a close follows the last by one epoch",
            close.epoch().get(),
            after.get()
        )));
    }
    let mut fetched = Vec::new();
    for version in close.catalog() {
        let contents = announced_catalog(peers, version).await.map_err(|error| {
            RoleError::unavailable(format!("a catalog version a close announces: {error}"))
        })?;
        fetched.push(contents);
    }
    for contents in fetched {
        // A core gone holds nothing to install.
        drop(to_core.unbounded_send(CoreMessage::Catalog(contents)));
    }
    let epoch = close.epoch();
    // A core gone holds nothing to deliver to.
    drop(closes.send(PartitionMessage::Close(close)));
    Ok(epoch)
}

/// What a subscription's stream brought.
enum Subscribed {
    /// A close.
    Close(Close),
    /// The subscription's end by the group's Error 1, what it said: the close it would send next is no longer held.
    Ended(String),
}

/// What `answer`, read for the Subscribe on the connection's last stream by [`RoleClient::next_answer`]'s rule, brought.
///
/// # Errors
///
/// The group's Error on stream 0, its connection error, or of a code other than 1 on the subscription's stream, a group
/// refusing as it stops, either joined again; and a close whose body breaks its rules, which this process refuses
/// (`PROTOCOL.md` §2.6, §13): the caller drops the client, which closes the connection.
fn subscribed(answer: Answered) -> Result<Subscribed, TransportError> {
    match answer {
        Answered::Frame(frame) => {
            close_of(&frame.view().map_err(TransportError::protocol)?).map(Subscribed::Close)
        }
        Answered::Refused {
            code: ErrorCode::Protocol,
            message,
        } => Ok(Subscribed::Ended(message)),
        Answered::Refused { code, message } | Answered::ConnectionRefused { code, message } => {
            Err(TransportError::refused(code, message))
        }
    }
}

/// The close `view` carries. A close always fits one frame (`PROTOCOL.md` §13.4 allows more, the group sends one).
fn close_of(view: &FrameView<'_>) -> Result<Close, TransportError> {
    let close = EpochCloseView::from_frame(view).map_err(TransportError::protocol)?;
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
/// A future dropped at a dial or a lease leaves the blocks supplied so far held; a block the group granted whose
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
            peers.system(),
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
/// supplied so far held; a block the group granted whose reply was not read goes unused, its sequences skipped.
async fn lease_ahead(
    client: &mut RoleClient,
    leased: &LeasedBlocks,
    own: PartitionId,
    prefix: TypePrefix,
) -> Result<(), TransportError> {
    // Ends once BLOCKS_AHEAD blocks are held: each pass supplies one, or returns the refusal (Ultra rule 2).
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

/// What the core asks of the system group, in turn on one connection: leases ahead of need, and closes it asks for.
///
/// A future dropped at its wait or a request drops the connection and what it was asking, which the core asks again: a
/// lease at its next allocation, a close request as the commit step holds on, a default entry by the next recovery.
async fn system_requests(
    peers: Peers,
    leased: LeasedBlocks,
    own: PartitionId,
    mut wants: UnboundedReceiver<TypePrefix>,
    mut asks: UnboundedReceiver<SystemAsk>,
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
            Asked::Frontend(SystemAsk::Report(report)) => {
                spawn(send_report(peers.clone(), report)).detach();
                continue;
            }
            Asked::Lease(_) | Asked::Frontend(SystemAsk::Close(_) | SystemAsk::DefaultEntry(_)) => {
                asked
            }
        };
        let held = match client.take() {
            Some(open) => Ok(open),
            None => {
                RoleClient::dial(
                    peers.system(),
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
                    Asked::Frontend(SystemAsk::Close(epoch)) => {
                        open.send_only(&CloseRequestBody::new(own.get(), epoch.get()))
                            .await
                    }
                    Asked::Frontend(SystemAsk::DefaultEntry(entry)) => {
                        let record =
                            DefaultEntry::request(entry.tenant(), entry.predicate().to_string());
                        open.request(&record).await.map(drop)
                    }
                    // Taken above, each on a connection of its own.
                    Asked::Frontend(SystemAsk::Report(_)) => Ok(()),
                };
                client = result.is_ok().then_some(open);
                result
            }
            Err(error) => Err(error),
        };
        if let Err(error) = done {
            // A lease is asked again as the next block is taken, a close request again as the commit step holds on, and a
            // default entry again by the next recovery (`PROTOCOL.md` §11.12).
            tracing::warn!(%error, "a request to the system group failed");
        }
    }
}

/// Sends `report` until the system group answers it, on a connection of its own so a wait for the group holds no lease, or
/// until the role's deadline, past which the group's own wait for it has ended (`PROTOCOL.md` §13.19).
///
/// A future dropped at a dial, the request or a wait drops the report, which the close delivered again checks again.
async fn send_report(peers: Peers, report: Report) {
    // A deadline past what the clock can name never passes: `join_deadline_ms` is a setting, so no sum of it may panic.
    let due = now().checked_add(peers.cluster().deadline());
    loop {
        let dialled = RoleClient::dial(
            peers.system(),
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
            Err(error) if due.is_some_and(|due| now() >= due) => {
                tracing::warn!(%error, version = report.version(), "a catalog version's report went unanswered");
                return;
            }
            Err(_) => compio::time::sleep(REPORT_AGAIN).await,
        }
    }
}

/// What the core's connection to the system group takes next.
enum Asked {
    Lease(TypePrefix),
    Frontend(SystemAsk),
}
