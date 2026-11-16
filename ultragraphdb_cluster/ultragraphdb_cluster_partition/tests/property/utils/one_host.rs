/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! One partition's host over a fault-free simulated store and log, epoch 1 closed: what the property tests drive. The unit
//! tests' fixture (`tests/utils/`) holds more than these use, so the property tree keeps this one of its own.

use std::future::Future;
use std::num::NonZeroU64;
use std::pin::pin;
use std::task::{Context as TaskContext, Poll, Waker};

use oxrdf::{GraphName, Literal, NamedNode};
use ultragraphdb_cluster_partition::{
    Partition, PartitionHost, PartitionMailbox, PartitionOutcome,
};
use ultragraphdb_cluster_placement::{NodeId, PartitionId, PlacementMap};
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_core_mapping::{Change, Changeset, RandomSource};
use ultragraphdb_store_apply::ChangesetApplier;
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_quads::QuadStore;
use ultragraphdb_store_sim::{
    FaultConfig, Seed, SimClock, SimListCache, SimStore, SteppedExecutor, Stream,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_traits_blocks::TestBlockSource;
use ultragraphdb_transactions_commit::{CloseRequest, Pipeline, RoundBounds};
use ultragraphdb_transactions_log::{SimStorage, Wal, WalConfig};
use ultragraphdb_transactions_txn::{Mode, TxnBuffers, TxnLimits};

/// The tenant the tests write.
pub(crate) const TENANT: u64 = 0;

/// How many polls a simulated log's future takes at most: it waits on nothing but its own steps.
const POLLS_MAX: u32 = 1_000_000;

type Part = Partition<SimStore, SimListCache<CachedList>, SteppedExecutor, TestBlockSource>;

/// The host the property tests drive.
pub(crate) type Host<'a> = PartitionHost<
    'a,
    Wal<SimStorage>,
    SimStore,
    SimListCache<CachedList>,
    SteppedExecutor,
    TestBlockSource,
>;

/// Runs `future` to its end, refused past [`POLLS_MAX`] polls.
fn block_on<F: Future>(future: F) -> TestResult<F::Output> {
    let mut future = pin!(future);
    let mut context = TaskContext::from_waker(Waker::noop());
    for _ in 0..POLLS_MAX {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return Ok(output);
        }
    }
    Err(TestError::new(
        "a future of the simulated log stayed pending",
    ))
}

/// The IRI `http://ex.org/<name>`.
pub(crate) fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

/// A changeset of [`TENANT`] inserting `<subject> <predicate> "value"` in the default graph and in `<graph>`.
pub(crate) fn written(subject: &str, predicate: &str, graph: &str) -> Changeset {
    let change = |graph_name: GraphName, value: &str| Change::Insert {
        graph: graph_name,
        subject: ex(subject).into(),
        predicate: ex(predicate),
        object: Literal::new_simple_literal(value).into(),
    };
    Changeset::new(TENANT)
        .with(change(GraphName::DefaultGraph, "v"))
        .with(change(GraphName::NamedNode(ex(graph)), "w"))
}

/// Partition 1 of one, over a fault-free simulated store, owning its buckets at `first`.
fn node(placement: &PlacementMap, first: Epoch) -> TestResult<Part> {
    let id = PartitionId::new(1)?;
    let seed = Seed::new(1);
    let store = SimStore::new(seed, FaultConfig::none(), SimClock::new());
    let cache = SimListCache::new(1024, seed.stream(Stream::Cache));
    let quads = QuadStore::open(
        Lists::new(store, cache, ListsConfig::default()),
        TestBlockSource::new(NonZeroU64::MIN.saturating_add(999)),
        RandomSource::new(1),
    )?
    .owning(Part::owned(id, placement, first).context("the map's ranges")?);
    let applier = ChangesetApplier::over(
        quads,
        SteppedExecutor::new(seed.stream(Stream::Executor)),
        1,
    )?;
    Ok(Partition::new(id, placement.clone(), applier))
}

/// Runs `body` on the host of partition 1 of one, epoch 1 closed.
pub(crate) fn on_host(body: impl FnOnce(&mut Host<'_>) -> TestResult) -> TestResult {
    let placement = PlacementMap::even(1, NodeId::new(1))?;
    let first = Epoch::new(1)?;
    let part = node(&placement, first)?;
    let storage = SimStorage::new(1);
    let (log, _) =
        block_on(Wal::open(storage, "/wal", WalConfig::new(1, true)))?.context("the log opens")?;
    let pipeline = Pipeline::new(part.applier(), log, RoundBounds::new(u32::MAX, u32::MAX))?
        .with_partition(1, first, CloseRequest::default(), &[])
        .bound(part.id(), part.placement());
    let mailbox = PartitionMailbox::mesh(&[part.id()])
        .into_iter()
        .next()
        .context("a mailbox")?;
    let mut host = PartitionHost::new(&part, pipeline, mailbox);
    body(&mut host)
}

/// Turns `host` `turns` times at time 0, and gives what ended.
pub(crate) fn turned(host: &mut Host<'_>, turns: usize) -> TestResult<Vec<PartitionOutcome>> {
    let mut ended = Vec::new();
    for _ in 0..turns {
        block_on(host.turn(0))??;
        ended.extend(host.take_outcomes());
    }
    Ok(ended)
}

/// Whether `host`'s partition holds `<subject> <predicate>` with some object in the default graph, read at its latest
/// state: what a Drop removed shows as no object.
pub(crate) fn holds(host: &Host<'_>, subject: &str, predicate: &str) -> TestResult<bool> {
    let mut txn = host.partition().begin(
        TENANT,
        Mode::ReadWrite,
        0,
        0,
        TxnLimits::default(),
        TxnBuffers::new(),
    )?;
    let mut objects = Vec::new();
    let read = txn.objects_into(
        GraphName::DefaultGraph.as_ref(),
        ex(subject).as_ref().into(),
        ex(predicate).as_ref(),
        0,
        &mut objects,
    );
    let _buffers: TxnBuffers = txn.abort();
    read?;
    Ok(!objects.is_empty())
}
