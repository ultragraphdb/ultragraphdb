/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;
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
    ROLE_ITERATIONS, RoleClient, RoleCluster, RoleCredentials, Server, frontend_address, listen,
    retry_until, serve_connection,
};
use ultragraphdb_common_service::{Readiness, RoleKind, RoleStart, Stopping, core_runtime};
use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_core_ids::{Epoch, Namespace, TypePrefix, Version};
use ultragraphdb_core_mapping::RandomSource;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::{Address, Endpoint, TransportError};
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_protocol_wire::{
    BodyView, CloseRequest as CloseRequestBody, EpochCloseView, Join, Lease, LeaseView, Placement,
    PlacementView, Subscribe,
};
use ultragraphdb_store_apply::ChangesetApplier;
use ultragraphdb_store_kv::{Durability, FjallStore, MokaListCache, ThreadExecutor};
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_quads::QuadStore;
use ultragraphdb_traits_blocks::SequenceBlock;
use ultragraphdb_transactions_commit::{CloseRequest, Pipeline, Replay, RoundBounds, Sessions};
use ultragraphdb_transactions_log::{CompioStorage, Wal, WalConfig};

use crate::types::core_message::CoreMessage;
use crate::types::partition_core::{
    CoreParts, FrontendAsk, PartitionCore, Peers, StorageHost, StoragePartition,
};
use crate::types::refill_blocks::RefillBlocks;
use crate::types::storage_session::StorageSession;
use crate::utils::partition_address::partition_address;

/// The posting lists' cache per partition.
const CACHE_BYTES: u64 = 64 << 20;

/// The sequences a core leases at a time, and how many blocks it keeps ahead.
const BLOCK_SIZE: u64 = 1 << 16;
const BLOCKS_AHEAD: usize = 2;

const POLL: std::time::Duration = std::time::Duration::from_millis(20);

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
pub async fn storage_role(
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
    let mut ended = Vec::new();
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
        ended.push(spawn_core(core_start, mailbox, joined, stopping.clone())?);
    }
    for is_ready in ready {
        let joined = wait_or_end(is_ready, &mut ended, &stopping).await?;
        if !joined {
            return finish(ended).await;
        }
    }
    readiness.set(true);
    tracing::info!(partitions = count, "ready");
    while !stopping.is_stopping() {
        for one in &mut ended {
            if let Ok(Some(outcome)) = one.try_recv() {
                outcome?;
            }
        }
        compio::time::sleep(POLL).await;
    }
    finish(ended).await
}

/// Waits for every core to end, each past its flush.
async fn finish(ended: Vec<oneshot::Receiver<Result<(), RoleError>>>) -> Result<(), RoleError> {
    for one in ended {
        one.await.unwrap_or_else(|_| {
            Err(RoleError::software(
                "a partition's thread ended without a word",
            ))
        })?;
    }
    Ok(())
}

/// Whether the core behind `is_ready` became ready; false once the service stops first. A core ending first ends this.
async fn wait_or_end(
    mut is_ready: oneshot::Receiver<()>,
    ended: &mut [oneshot::Receiver<Result<(), RoleError>>],
    stopping: &Stopping,
) -> Result<bool, RoleError> {
    loop {
        if matches!(is_ready.try_recv(), Ok(Some(()))) {
            return Ok(true);
        }
        for one in ended.iter_mut() {
            if let Ok(Some(outcome)) = one.try_recv() {
                outcome?;
                return Ok(false);
            }
        }
        if stopping.is_stopping() {
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
async fn serve_core(
    start: CoreStart,
    mailbox: PartitionMailbox,
    joined: oneshot::Sender<()>,
    stopping: Stopping,
) -> Result<(), RoleError> {
    let name = RoleKind::Storage.to_string();
    let cluster = &start.cluster;
    let Some(placement) = retry_until(cluster, &name, &stopping, || {
        placement_of(&start.frontend, cluster, &name)
    })
    .await?
    else {
        return Ok(());
    };
    let own = start.id;
    let dir = start.data_dir.join(format!("p{}", own.get()));
    std::fs::create_dir_all(&dir)
        .map_err(|error| RoleError::io(format!("the data directory {}: {error}", dir.display())))?;
    let leased = LeasedBlocks::new();
    let (wanted, wants) = unbounded();
    let blocks = RefillBlocks::new(leased.clone(), wanted);
    let partition = open_partition(&dir.join("store"), own, &placement, blocks)?;
    let config = WalConfig::new(cluster.id(), false);
    let bounds = RoundBounds::new(config.round_bytes(), config.round_transactions());
    let (mut log, _) = Wal::open(CompioStorage, dir.join("wal"), config)
        .await
        .map_err(|error| RoleError::io(format!("the partition {} log: {error}", own.get())))?;
    let replay = Replay::run_in(
        own.get(),
        partition.applier(),
        &mut log,
        Sessions::DEFAULT_CAPACITY,
    )
    .await
    .map_err(|error| RoleError::io(format!("the partition {} replay: {error}", own.get())))?;
    tracing::info!(partition = own.get(), rounds = replay.rounds(), "recovered");
    let delivered = last_close(replay.applied());
    let pipeline = Pipeline::new(partition.applier(), log, bounds)
        .map_err(|error| RoleError::io(format!("the pipeline: {error}")))?
        .with_partition(own.get(), delivered, close_request(&start.config)?, &[])
        .bound(own, partition.placement());
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
    };
    serve_partition(&start, served, &stopping).await
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
}

/// Listens at the partition's address, leases its first blocks, joins the frontend and subscribes to its closes, then runs
/// the partition's core until the service stops or the frontend stays away past the deadline.
async fn serve_partition(
    start: &CoreStart,
    serving: Serving<'_>,
    stopping: &Stopping,
) -> Result<(), RoleError> {
    let own = start.id;
    let cluster = &start.cluster;
    let name = RoleKind::Storage.to_string();
    let address = partition_address(&start.base, start.first, own.get())?;
    let (to_core, inbox) = unbounded();
    accept_connections(listen(&address).await?, start, to_core)?;
    let (asks_tx, asks) = unbounded();
    let (closes_tx, closes) = std::sync::mpsc::channel();
    let (answers_tx, answers) = unbounded();
    let peers = Peers {
        cluster: cluster.clone(),
        frontend: start.frontend.clone(),
        name: name.clone(),
        own: answers_tx,
        addresses: Rc::new(RefCell::new(BTreeMap::new())),
        open: Rc::new(RefCell::new(HashMap::new())),
    };
    spawn(forward_answers(answers, serving.self_sender.clone())).detach();
    top_up(&peers, &serving.leased, own, TypePrefix::RESOURCE, stopping).await?;
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
    let (lost_tx, mut lost) = oneshot::channel();
    spawn(deliver_closes(
        subscription,
        peers.clone(),
        join,
        serving.delivered,
        closes_tx,
        lost_tx,
        stopping.clone(),
    ))
    .detach();
    let core = PartitionCore::new(CoreParts {
        host: serving.host,
        id: own,
        cluster: cluster.id(),
        clock: now(),
        inbox,
        remote: serving.remote,
        own: serving.self_sender,
        closes,
        asks: asks_tx,
        peers,
    });
    let mut running = std::pin::pin!(core.run(stopping));
    std::future::poll_fn(|cx| {
        if let std::task::Poll::Ready(Ok(failure)) = std::pin::Pin::new(&mut lost).poll(cx) {
            return std::task::Poll::Ready(Err(failure));
        }
        running.as_mut().poll(cx)
    })
    .await
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
    let server = Server::new(start.cluster.id(), "storage", Endpoint::Partition)?
        .with_iterations(ROLE_ITERATIONS);
    spawn(async move {
        let mut connection = 0u64;
        loop {
            match listener.accept().await {
                Ok(carrier) => {
                    connection += 1;
                    let session = StorageSession::new(connection, core.clone());
                    let (server, credentials) = (server.clone(), credentials.clone());
                    spawn(async move {
                        serve_connection(carrier, &server, credentials, session).await;
                    })
                    .detach();
                }
                Err(error) => tracing::warn!(%error, "a role connection was not accepted"),
            }
        }
    })
    .detach();
    Ok(())
}

/// The map the frontend holds, checked against the even map this stage holds fixed.
async fn placement_of(
    frontend: &Address,
    cluster: &RoleCluster,
    name: &str,
) -> Result<PlacementMap, TransportError> {
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
    Ok(map)
}

/// Joins for this partition and subscribes from `from`: the connection the closes arrive on.
async fn joined_subscription(
    peers: &Peers,
    join: &Join,
    from: Epoch,
) -> Result<RoleClient, TransportError> {
    let mut client = RoleClient::dial(
        &peers.frontend,
        peers.cluster.id(),
        &peers.name,
        peers.cluster.secret().password(),
    )
    .await?;
    drop(client.request(join).await?);
    client.send_only(&Subscribe::new(from.get())).await?;
    Ok(client)
}

/// Hands each close the subscription brings to the partition's mailbox, joining and subscribing again when the frontend
/// goes away, from the last close it holds, until the deadline; past it, `lost` ends the core.
async fn deliver_closes(
    mut client: RoleClient,
    peers: Peers,
    join: Join,
    mut after: Epoch,
    closes: std::sync::mpsc::Sender<PartitionMessage>,
    lost: oneshot::Sender<RoleError>,
    stopping: Stopping,
) {
    while !stopping.is_stopping() {
        let read = client.next_frame().await;
        let close = read.and_then(|frame| close_of(&frame));
        match close {
            Ok(close) => {
                after = close.epoch();
                // A core gone holds nothing to deliver to.
                drop(closes.send(PartitionMessage::Close(close)));
            }
            Err(error) => {
                tracing::warn!(%error, "the frontend's connection ended; joining again");
                let name = peers.name.clone();
                let cluster = peers.cluster.clone();
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
        .map(|announced| {
            CatalogVersion::new(Namespace::new(announced.namespace()), announced.version())
        })
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
async fn forward_answers(
    mut answers: UnboundedReceiver<PartitionMessage>,
    own: std::sync::mpsc::Sender<PartitionMessage>,
) {
    while let Some(answer) = answers.next().await {
        // A mailbox gone is a partition stopping.
        drop(own.send(answer));
    }
}

/// Leases until `prefix` is held ahead of need, retrying until the deadline.
async fn top_up(
    peers: &Peers,
    leased: &LeasedBlocks,
    own: PartitionId,
    prefix: TypePrefix,
    stopping: &Stopping,
) -> Result<(), RoleError> {
    let name = peers.name.clone();
    let _done = retry_until(&peers.cluster, &name, stopping, || async {
        let mut client = RoleClient::dial(
            &peers.frontend,
            peers.cluster.id(),
            &peers.name,
            peers.cluster.secret().password(),
        )
        .await?;
        lease_ahead(&mut client, leased, own, prefix).await
    })
    .await?;
    Ok(())
}

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
async fn frontend_requests(
    peers: Peers,
    leased: LeasedBlocks,
    own: PartitionId,
    mut wants: UnboundedReceiver<TypePrefix>,
    mut asks: UnboundedReceiver<FrontendAsk>,
) {
    let mut client: Option<RoleClient> = None;
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
        let held = match client.take() {
            Some(open) => Ok(open),
            None => {
                RoleClient::dial(
                    &peers.frontend,
                    peers.cluster.id(),
                    &peers.name,
                    peers.cluster.secret().password(),
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
                };
                client = result.is_ok().then_some(open);
                result
            }
            Err(error) => Err(error),
        };
        if let Err(error) = done {
            // A lease is asked again as the next block is taken, and a close request again as the commit step holds on.
            tracing::warn!(%error, "a request to the frontend failed");
        }
    }
}

/// What the core's frontend connection takes next.
enum Asked {
    Lease(TypePrefix),
    Frontend(FrontendAsk),
}
