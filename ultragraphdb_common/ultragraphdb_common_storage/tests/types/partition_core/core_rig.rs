/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A partition's core in the test's process for the reads', the versions' and the one-frame writes' tests: partition 1 of a
//! one-partition map over fjall and its write-ahead log in a directory of its own, listening on a loopback address where each
//! connection is a [`StorageSession`], served by [`PartitionCore::run`] beside the test until the test ends. The channels the
//! role's shell holds, the closes it delivers, what the core asks of the frontend and the inbox itself, are the test's.

use std::sync::mpsc::Sender;
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use ultragraphdb_cluster_metadata::LeasedBlocks;
use ultragraphdb_cluster_partition::{
    Partition, PartitionHost, PartitionMailbox, PartitionMessage,
};
use ultragraphdb_cluster_placement::{NodeId, PartitionId, PlacementMap};
use ultragraphdb_common_role::{
    PARTITION_INITIAL_CREDIT, ROLE_ITERATIONS, RoleClient, RoleCluster, RoleSecret, Server, listen,
    serve_connection,
};
use ultragraphdb_common_service::Stopping;
use ultragraphdb_common_storage::{
    CoreChannels, CoreMessage, CoreParts, FrontendAsk, PartitionCore, Peers, RefillBlocks,
    Retention, StoragePartition, StorageSession,
};
use ultragraphdb_core_ids::{Epoch, TypePrefix};
use ultragraphdb_core_mapping::RandomSource;
use ultragraphdb_protocol_transport::{Address, Endpoint};
use ultragraphdb_store_apply::ChangesetApplier;
use ultragraphdb_store_kv::{Durability, FjallStore, MokaListCache, ThreadExecutor};
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_quads::QuadStore;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_test::TestDir;
use ultragraphdb_traits_blocks::SequenceBlock;
use ultragraphdb_transactions_commit::{CloseRequest, Pipeline, RoundBounds};
use ultragraphdb_transactions_log::{CompioStorage, Wal, WalConfig};

use crate::types::partition_core::yielding_session::Yielding;

/// The cluster every frame names.
pub(crate) const CLUSTER: u64 = 7;
/// The partition the core hosts.
pub(crate) const PARTITION: u32 = 1;
/// The tenant the placement map lists a run for.
pub(crate) const TENANT: u64 = 0;
/// The principal the tests write and read as.
pub(crate) const PRINCIPAL: u64 = 1;
/// How long a test waits for any one frame or answer.
pub(crate) const DUE: Duration = Duration::from_secs(10);
/// The role secret every connection authenticates with.
const PASSWORD: &str = "s3cret";
/// The posting lists' cache.
const CACHE_BYTES: u64 = 1 << 20;
/// The bytes each connection may hold parked.
const HELD_BYTES: usize = 1 << 20;
/// The sequences of the one block of each type leased: more than a test allocates.
const BLOCK: u64 = 1 << 16;

/// How long the rig's core keeps a Scan with no frame able to leave, in milliseconds, and the tenants it takes Binds of.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RigSetup {
    pub(crate) stall_ms: u64,
    pub(crate) tenant: u64,
}

impl Default for RigSetup {
    fn default() -> Self {
        Self {
            stall_ms: 30_000,
            tenant: TENANT,
        }
    }
}

/// What a test reaches its running core through.
#[derive(Debug)]
pub(crate) struct Rig {
    address: Address,
    /// The closes the role's shell delivers to the core.
    pub(crate) closes: Sender<PartitionMessage>,
    /// What the core asks of the frontend.
    pub(crate) asks: UnboundedReceiver<FrontendAsk>,
    /// The core's inbox, which its connections hand their requests to.
    pub(crate) inbox: UnboundedSender<CoreMessage>,
}

impl Rig {
    /// A connection to the core, dialled and authenticated as compute dials one.
    pub(crate) async fn dial(&self) -> TestResult<RoleClient> {
        Ok(RoleClient::dial(&self.address, CLUSTER, "compute", PASSWORD).await?)
    }

    /// The next report the core sends the frontend, within [`DUE`], past what else it asks.
    pub(crate) async fn report(&mut self) -> TestResult<ultragraphdb_protocol_wire::Report> {
        let asks = &mut self.asks;
        compio::time::timeout(DUE, async {
            while let Some(ask) = asks.next().await {
                if let FrontendAsk::Report(report) = ask {
                    return Some(report);
                }
            }
            None
        })
        .await
        .context("a report within its time")?
        .context("the core holds its asks")
    }
}

/// Runs `test` beside a core set up as `setup` until `test` ends; then the core stops, as a role stopping stops it: `test`'s
/// outcome, or the core's failure.
pub(crate) fn with_core(
    name: &str,
    setup: RigSetup,
    test: impl AsyncFnOnce(Rig) -> TestResult,
) -> TestResult {
    with_core_then(name, setup, test, async |_| Ok(()))
}

/// Runs `test` as [`with_core`] does, then, once the core has stopped and let go of its write-ahead log, `after` over the
/// directory of that log.
pub(crate) fn with_core_then(
    name: &str,
    setup: RigSetup,
    test: impl AsyncFnOnce(Rig) -> TestResult,
    after: impl AsyncFnOnce(std::path::PathBuf) -> TestResult,
) -> TestResult {
    let dir = TestDir::new(name);
    Runtime::new()?.block_on(async {
        let placement = PlacementMap::even(1, NodeId::new(1))?;
        let id = PartitionId::new(PARTITION)?;
        let (wanted, _wants) = unbounded();
        let blocks = RefillBlocks::new(leased()?, wanted);
        let partition = open_partition(dir.path(), id, &placement, blocks)?;
        let config = WalConfig::new(CLUSTER, false);
        let (log, _) = Wal::open(CompioStorage, dir.path().join("wal"), config).await?;
        let pipeline = Pipeline::new(partition.applier(), log, RoundBounds::new(1 << 20, 1 << 10))?
            .with_partition(PARTITION, Epoch::ZERO, CloseRequest::default(), &[])
            .bound(id, partition.placement());
        let (remote_tx, remote) = std::sync::mpsc::channel();
        let mailbox = PartitionMailbox::mesh(&[id])
            .into_iter()
            .next()
            .context("the partition's mailbox")?
            .with_remote(remote_tx);
        let own = mailbox.sender(id).context("its mailbox reaches itself")?;
        let host = PartitionHost::new(&partition, pipeline, mailbox).with_first_ticket(1);
        let port = std::net::TcpListener::bind("127.0.0.1:0")?
            .local_addr()?
            .port();
        let address = Address::parse(&format!("tcp://127.0.0.1:{port}"))?;
        let (inbox_tx, inbox) = unbounded();
        let cluster = role_cluster(dir.path())?;
        accept(&address, &cluster, inbox_tx.clone()).await?;
        let (asks_tx, asks) = unbounded();
        let (closes_tx, closes) = std::sync::mpsc::channel();
        let (discards, _discarded) = std::sync::mpsc::channel();
        let (answers, _answered) = unbounded();
        let frontend = Address::parse("tcp://127.0.0.1:1")?;
        let peers = Peers::new(cluster, frontend, "storage".to_string(), answers);
        let channels = CoreChannels::new(inbox, remote, own, closes, asks_tx, discards);
        // The core's clock is the runtime's: its timer's first tick is now.
        let clock = compio::time::interval(Duration::from_secs(1)).tick().await;
        let retention = Retention::new(60_000);
        let parts = CoreParts::new(host, id, CLUSTER, clock, channels, peers, retention)
            .with_scan_stall_ms(setup.stall_ms)
            .with_tenants([setup.tenant]);
        let stopping = Stopping::default();
        let rig = Rig {
            address,
            closes: closes_tx,
            asks,
            inbox: inbox_tx,
        };
        let tested = async {
            let outcome = test(rig).await;
            stopping.set();
            outcome
        };
        let core = PartitionCore::new(parts);
        let (ran, tested) = futures::future::join(core.run(&stopping), tested).await;
        tested?;
        ran?;
        after(dir.path().join("wal")).await
    })
}

/// One block of each type the partition allocates.
fn leased() -> TestResult<LeasedBlocks> {
    let leased = LeasedBlocks::new();
    for prefix in [TypePrefix::RESOURCE, TypePrefix::STATEMENT] {
        leased.supply(prefix, SequenceBlock::new(1, BLOCK).context("a block")?)?;
    }
    Ok(leased)
}

/// The role cluster, its secret read from a file in `dir`.
fn role_cluster(dir: &std::path::Path) -> TestResult<RoleCluster> {
    std::fs::create_dir_all(dir)?;
    let file = dir.join("role-secret");
    std::fs::write(&file, PASSWORD)?;
    let secret = RoleSecret::read(&file.display().to_string())?;
    Ok(RoleCluster::new(CLUSTER, secret, Duration::from_secs(1)))
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
