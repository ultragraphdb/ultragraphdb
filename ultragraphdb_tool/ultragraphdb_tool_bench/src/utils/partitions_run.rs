/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The measurement of N partitions in one process (partitions, "The benchmark measures N partitions in one process"):
//! each on a pinned compio runtime with a store and a log of its own and a generator of its own, exchanging registrations
//! as messages, while this thread stands in for the sequencer, closing an epoch every [`EPOCH`].

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

use oxrdf::{GraphName, NamedNode, Term};
use ultragraphdb_cluster_partition::{
    Fanout, Partition, PartitionHost, PartitionMailbox, PartitionMessage, ReverseQuery,
};
use ultragraphdb_cluster_placement::{NodeId, PartitionId, PlacementMap};
use ultragraphdb_cluster_sequencer::Close;
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_core_keys::Graph;
use ultragraphdb_core_mapping::{Change, Changeset};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_kv::{Durability, ThreadExecutor};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_store_quads::iri_bucket;
use ultragraphdb_traits_blocks::TestBlockSource;
use ultragraphdb_traits_log::OrderedLog;
use ultragraphdb_transactions_commit::{CloseRequest, Pipeline, RoundBounds};
use ultragraphdb_transactions_log::{CompioStorage, Wal, WalConfig};

use crate::errors::bench_error::BenchError;
use crate::traits::clock::Clock;
use crate::types::counting_cache::CountingCache;
use crate::types::dir_sizes::DirSizes;
use crate::types::pacer::Pacer;
use crate::types::partition_args::PartitionArgs;
use crate::types::partition_shape::PartitionShape;
use crate::types::summary::Summary;
use crate::types::timed_store::TimedStore;
use crate::types::wall_clock::WallClock;
use crate::utils::pipeline_run::wal_files;
use crate::utils::setup::{open_applier, open_partition, size_json};
use ultragraphdb_store_kv::MokaListCache;

/// A partition of the bench, over fjall.
type BenchPartition = Partition<
    TimedStore,
    CountingCache<MokaListCache<CachedList>>,
    ThreadExecutor,
    TestBlockSource,
>;

/// How often the sequencer's stand-in closes an epoch: the `dev` profile's longest.
const EPOCH: Duration = Duration::from_millis(10);
/// The writes a partition holds unended, Dgraph's limit on mutation proposals in flight, as in the other runs.
const PENDING: u64 = 256;
/// The cluster the logs' rounds name.
const CLUSTER: u64 = 1;
/// The tenant the commits write, the root.
const TENANT: u64 = 0;
/// How long an idle partition waits before its next turn.
const IDLE: Duration = Duration::from_micros(50);

/// What the partitions share with the sequencer's stand-in: how many have not ended their writes, and the last close,
/// 0 until it is sent. `AcqRel` on a change, `Release` on the close's store and `Acquire` on its load suffice: a
/// partition that reads the close sees what the stand-in wrote before it.
#[derive(Debug)]
struct Shared {
    writers: AtomicUsize,
    last_close: AtomicU64,
}

/// What one partition counted.
#[derive(Debug, Default)]
struct Stats {
    committed: u64,
    refused: u64,
    registrations: u64,
    waits_us: Vec<u64>,
    rounds: u64,
    /// The latest close delivered when the partition ended.
    delivered: u64,
    /// The writes committed, when the run verifies.
    kept: Vec<Changeset>,
}

/// What a partition's thread takes.
struct Worker {
    part: Arc<BenchPartition>,
    wal_dir: PathBuf,
    shape: PartitionShape,
    cpu: usize,
    clock: WallClock,
    shared: Arc<Shared>,
    rate: u64,
    seconds: u64,
    commits: u64,
    verify: bool,
}

/// Runs the measurement of `args` and returns its report, one JSON line.
///
/// # Errors
///
/// [`BenchError::usage`] when the run's directory exists; the store's, the applier's, the log's or the pipeline's error,
/// which ends the run; [`BenchError::commit`] when a reverse read differs from the same commits' on one partition.
pub fn run_partitions(args: &PartitionArgs) -> Result<String, BenchError> {
    if args.dir().exists() {
        return Err(BenchError::usage(format!(
            "{} exists; every run takes a fresh directory",
            args.dir().display()
        )));
    }
    std::fs::create_dir_all(args.dir())?;
    let placement = PlacementMap::even(args.partitions(), NodeId::new(1))
        .map_err(|e| BenchError::usage(e.to_string()))?;
    // Partitions 1 to the count, half-open: `even` refused a count above `PARTITIONS_MAX`, so the end cannot overflow.
    let ids = (1..args.partitions() + 1)
        .map(PartitionId::new)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| BenchError::usage(e.to_string()))?;
    let parts = ids
        .iter()
        .map(|id| open_part(args, *id, &placement))
        .collect::<Result<Vec<_>, _>>()?;
    let mailboxes = PartitionMailbox::mesh(&ids);
    let closers = mailboxes
        .iter()
        .filter_map(|m| m.sender(m.id()))
        .collect::<Vec<_>>();
    let shared = Arc::new(Shared {
        writers: AtomicUsize::new(ids.len()),
        last_close: AtomicU64::new(0),
    });
    let clock = WallClock::new();
    let threads = spawn_workers(args, &parts, mailboxes, &placement, clock, &shared)?;
    let (last_close, elapsed) = close_until_done(&closers, &shared, clock)?;
    let stats = threads
        .into_iter()
        .map(|thread| {
            thread
                .join()
                .map_err(|_| BenchError::commit("a partition's thread panicked".to_string()))?
        })
        .collect::<Result<Vec<_>, _>>()?;
    let verified = if args.verify() {
        Some(verify(args, &parts, &stats, last_close)?)
    } else {
        None
    };
    report(args, &parts, &stats, elapsed, last_close, verified)
}

/// Partition `id`'s store, in a directory of its own, over the buckets `placement` gives it.
fn open_part(
    args: &PartitionArgs,
    id: PartitionId,
    placement: &PlacementMap,
) -> Result<Arc<BenchPartition>, BenchError> {
    let owned = BenchPartition::owned(id, placement, Epoch::ZERO)
        .ok_or_else(|| BenchError::usage("a placement map's ranges overlap".to_string()))?;
    let applier = open_partition(&store_dir(args, id), args.cache_mb(), owned)?;
    Ok(Arc::new(Partition::new(id, placement.clone(), applier)))
}

fn store_dir(args: &PartitionArgs, id: PartitionId) -> PathBuf {
    args.dir().join(format!("p{}", id.get())).join("store")
}

fn wal_dir(args: &PartitionArgs, id: PartitionId) -> PathBuf {
    args.dir().join(format!("p{}", id.get())).join("wal")
}

/// One thread per partition, pinned in turn to the machine's CPUs.
#[expect(
    clippy::disallowed_methods,
    reason = "each partition runs on a thread and a runtime of its own, as the storage process runs them"
)]
fn spawn_workers(
    args: &PartitionArgs,
    parts: &[Arc<BenchPartition>],
    mailboxes: Vec<PartitionMailbox>,
    placement: &PlacementMap,
    clock: WallClock,
    shared: &Arc<Shared>,
) -> Result<Vec<std::thread::JoinHandle<Result<Stats, BenchError>>>, BenchError> {
    let cpus = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let mut threads = Vec::with_capacity(parts.len());
    for (at, (part, mailbox)) in parts.iter().zip(mailboxes).enumerate() {
        let id = part.id();
        let shape = PartitionShape::new(
            args.seed(),
            id,
            placement,
            args.relations(),
            args.cross_percent(),
        )
        .ok_or_else(|| BenchError::usage("a generator found no hubs".to_string()))?;
        std::fs::create_dir_all(wal_dir(args, id))?;
        let worker = Worker {
            part: Arc::clone(part),
            wal_dir: wal_dir(args, id),
            shape,
            cpu: at % cpus,
            clock,
            shared: Arc::clone(shared),
            rate: args.rate(),
            seconds: args.seconds(),
            commits: args.commits(),
            verify: args.verify(),
        };
        threads.push(std::thread::spawn(move || serve(worker, mailbox)));
    }
    Ok(threads)
}

/// Closes an epoch every [`EPOCH`] until every partition has ended its writes, then a last one: that close and the run's
/// length.
fn close_until_done(
    closers: &[Sender<PartitionMessage>],
    shared: &Shared,
    clock: WallClock,
) -> Result<(Epoch, Duration), BenchError> {
    let send = |epoch: Epoch| -> Result<(), BenchError> {
        let close = Close::new(epoch, 0, Vec::new(), Vec::new())
            .map_err(|e| BenchError::commit(e.to_string()))?;
        for closer in closers {
            // A partition gone has failed; its thread's result says why.
            drop(closer.send(PartitionMessage::Close(close.clone())));
        }
        Ok(())
    };
    let next = |epoch: Epoch| epoch.next().map_err(|e| BenchError::commit(e.to_string()));
    let mut epoch = Epoch::new(1).map_err(|e| BenchError::commit(e.to_string()))?;
    // Ends when every partition has ended its writes, which each does at its commits or the run's length, or failing.
    while shared.writers.load(Ordering::Acquire) > 0 {
        std::thread::sleep(EPOCH);
        epoch = next(epoch)?;
        send(epoch)?;
    }
    let elapsed = clock.elapsed();
    let last = next(epoch)?;
    send(last)?;
    shared.last_close.store(last.get(), Ordering::Release);
    Ok((last, elapsed))
}

/// A partition's thread: its runtime, its log, its host and its writes.
fn serve(worker: Worker, mailbox: PartitionMailbox) -> Result<Stats, BenchError> {
    let runtime = compio::runtime::Runtime::builder()
        .thread_affinity(HashSet::from([worker.cpu]))
        .build()?;
    let shared = Arc::clone(&worker.shared);
    let result = runtime.block_on(write_loop(worker, mailbox));
    if result.is_err() {
        // A failed partition counts as ended, so the others and the sequencer's stand-in go on to the end of the run.
        let _before: usize = shared.writers.fetch_sub(1, Ordering::AcqRel);
    }
    result
}

/// Offers the partition's commits at its rate, up to its commits or the run's length, then serves the others until the
/// last close is delivered. Its thread's `block_on` runs it to its end; a future dropped at an await would drop the
/// partition's host and log with it, the log as its last sync left it.
async fn write_loop(mut worker: Worker, mailbox: PartitionMailbox) -> Result<Stats, BenchError> {
    let part = Arc::clone(&worker.part);
    let config = WalConfig::new(CLUSTER, false);
    let bounds = RoundBounds::new(config.round_bytes(), config.round_transactions());
    let (log, _) = Wal::open(CompioStorage, &worker.wal_dir, config)
        .await
        .map_err(|e| BenchError::commit(e.to_string()))?;
    let first = Epoch::new(1).map_err(|e| BenchError::commit(e.to_string()))?;
    let pipeline = Pipeline::new(part.applier(), log, bounds)
        .map_err(|e| BenchError::commit(e.to_string()))?
        .with_partition(part.id().get(), first, CloseRequest::default(), &[])
        .bound(part.id(), part.placement());
    let mut host = PartitionHost::new(&part, pipeline, mailbox);
    let mut stats = Stats::default();
    let mut pacer = Pacer::new(worker.rate);
    let mut writes: BTreeMap<u64, Changeset> = BTreeMap::new();
    let (mut offered, mut owed, mut unended) = (0u64, 0u64, 0u64);
    let mut ended_writes = false;
    let deadline = Duration::from_secs(worker.seconds);
    // Ends when the last close has reached a partition that has ended its writes; the commit count or the deadline ends
    // those, and the stand-in closes until they have ended.
    loop {
        let now = micros(&worker.clock);
        let writing = if worker.commits > 0 {
            offered < worker.commits
        } else {
            worker.clock.elapsed() < deadline
        };
        if writing && !ended_writes {
            owed += pacer.take_due(&worker.clock);
            while owed > 0 && unended < PENDING && (worker.commits == 0 || offered < worker.commits)
            {
                let commit = worker.shape.next_commit();
                let ticket = host
                    .submit(commit.clone(), now)
                    .map_err(|e| BenchError::commit(e.to_string()))?;
                if worker.verify {
                    let _previous = writes.insert(ticket, commit);
                }
                offered += 1;
                unended += 1;
                owed -= 1;
            }
        }
        host.turn(now)
            .await
            .map_err(|e| BenchError::commit(e.to_string()))?;
        let ended = host.take_outcomes();
        let progressed = !ended.is_empty();
        for outcome in ended {
            unended -= 1;
            let commit = writes.remove(&outcome.ticket());
            count(&mut stats, &outcome, commit);
        }
        if !writing && unended == 0 && !ended_writes {
            ended_writes = true;
            let _before: usize = worker.shared.writers.fetch_sub(1, Ordering::AcqRel);
        }
        let last = worker.shared.last_close.load(Ordering::Acquire);
        if ended_writes && last > 0 && host.delivered().get() >= last {
            break;
        }
        if !progressed {
            compio::time::sleep(IDLE).await;
        }
    }
    stats.delivered = host.delivered().get();
    let (_, used) = host.into_parts();
    stats.rounds = used.committed().get();
    Ok(stats)
}

fn count(
    stats: &mut Stats,
    outcome: &ultragraphdb_cluster_partition::PartitionOutcome,
    commit: Option<Changeset>,
) {
    if outcome.outcome().is_err() {
        stats.refused += 1;
        return;
    }
    stats.committed += 1;
    if outcome.registrations() > 0 {
        stats.registrations += outcome.registrations() as u64; // a `usize` count, lossless
        stats.waits_us.push(outcome.waited_us());
    }
    stats.kept.extend(commit);
}

fn micros(clock: &WallClock) -> u64 {
    u64::try_from(clock.elapsed().as_micros()).unwrap_or(u64::MAX)
}

/// Replays the commits the partitions committed on one partition, and compares each edge's object that another partition's
/// subject points at: a reverse read at the run's last closed epoch returns the subjects one partition returns. How many
/// reads compared.
fn verify(
    args: &PartitionArgs,
    parts: &[Arc<BenchPartition>],
    stats: &[Stats],
    last_close: Epoch,
) -> Result<u64, BenchError> {
    let one = open_applier(&args.dir().join("one"), Durability::Log, args.cache_mb())?;
    let mut crossed: BTreeSet<(NamedNode, NamedNode)> = BTreeSet::new();
    for kept in stats.iter().flat_map(|s| &s.kept) {
        let _applied = one.apply(kept)?;
        crossed.extend(kept.changes().iter().filter_map(cross_edge(parts)));
    }
    let refs: Vec<&BenchPartition> = parts.iter().map(Arc::as_ref).collect();
    let mut compared = 0;
    for (predicate, object) in crossed {
        let home = owner(parts, &object).ok_or_else(|| no_home(&object))?;
        let found = home.applier().quads();
        let (Some(o), Some(p)) = (
            found.id_of(TENANT, object.as_str())?,
            owner(parts, &predicate)
                .ok_or_else(|| no_home(&predicate))?
                .applier()
                .quads()
                .id_of(TENANT, predicate.as_str())?,
        ) else {
            return Err(BenchError::commit(format!(
                "{object} has no ID at its home"
            )));
        };
        let query = ReverseQuery::new(home.id(), TENANT, Graph::Default, p, o, last_close);
        let mut subjects = Vec::new();
        let _asked: Vec<PartitionId> = Fanout::over(refs.clone())
            .subjects(&query, &mut subjects)
            .map_err(|e| BenchError::commit(e.to_string()))?;
        let mut names = BTreeSet::new();
        for subject in subjects {
            let holder = parts
                .iter()
                .find(|part| {
                    part.placement().partition_of(subject.bucket(), Epoch::ZERO) == part.id()
                })
                .ok_or_else(|| BenchError::commit("a subject has no partition".to_string()))?;
            let (iri, _) = holder.applier().quads().iri_of(TENANT, subject)?;
            let _new = names.insert(iri);
        }
        if names != one_subjects(&one, &predicate, &object)? {
            return Err(BenchError::commit(format!(
                "the reverse read of {object} through {predicate} differs from one partition's"
            )));
        }
        compared += 1;
    }
    Ok(compared)
}

/// The filter of a change for an edge whose object another partition than its subject's homes.
fn cross_edge(
    parts: &[Arc<BenchPartition>],
) -> impl Fn(&Change) -> Option<(NamedNode, NamedNode)> + '_ {
    move |change| {
        let Change::Insert {
            graph: GraphName::DefaultGraph,
            subject: oxrdf::NamedOrBlankNode::NamedNode(subject),
            predicate,
            object: Term::NamedNode(object),
        } = change
        else {
            return None;
        };
        let (from, to) = (owner(parts, subject)?, owner(parts, object)?);
        (from.id() != to.id()).then(|| (predicate.clone(), object.clone()))
    }
}

/// The partition homing `iri`.
fn owner<'p>(parts: &'p [Arc<BenchPartition>], iri: &NamedNode) -> Option<&'p BenchPartition> {
    let bucket = iri_bucket(TENANT, iri.as_str())?;
    parts
        .iter()
        .find(|part| part.placement().partition_of(bucket, Epoch::ZERO) == part.id())
        .map(Arc::as_ref)
}

fn no_home(iri: &NamedNode) -> BenchError {
    BenchError::commit(format!("{iri} has no partition"))
}

/// The subjects holding `predicate` to `object` on the one partition holding every commit.
fn one_subjects(
    one: &crate::utils::setup::BenchApplier,
    predicate: &NamedNode,
    object: &NamedNode,
) -> Result<BTreeSet<String>, BenchError> {
    let quads = one.quads();
    let term = |iri: &NamedNode| -> Result<_, BenchError> {
        quads
            .id_of(TENANT, iri.as_str())?
            .ok_or_else(|| BenchError::commit(format!("{iri} has no ID on one partition")))
    };
    let (o, p) = (term(object)?, term(predicate)?);
    quads
        .subjects(TENANT, Graph::Default, p, &EncodedTerm::id(o), u64::MAX)?
        .into_iter()
        .map(|id| Ok(quads.iri_of(TENANT, id)?.0))
        .collect()
}

/// The run's report: each partition's counts and rates, and the totals.
fn report(
    args: &PartitionArgs,
    parts: &[Arc<BenchPartition>],
    stats: &[Stats],
    elapsed: Duration,
    last_close: Epoch,
    verified: Option<u64>,
) -> Result<String, BenchError> {
    let secs = elapsed.as_secs_f64();
    // A `usize` count into a `u64`, lossless; each `as f64` below turns a count into a figure of the report: exact
    // below 2^53, rounded past it, which a rate printed to a few digits can take.
    let relations = |s: &Stats| s.committed * args.relations() as u64;
    let mut partitions = Vec::with_capacity(parts.len());
    let mut total_bytes = 0u64;
    for (part, s) in parts.iter().zip(stats) {
        let id = part.id();
        let quads = part.applier().quads();
        quads.lists().sync()?;
        let data = quads.lists().store().inner().data_path().to_path_buf();
        let store = DirSizes::of(&store_dir(args, id), &data)?;
        let wal = wal_files(&wal_dir(args, id), true)?.plus(wal_files(&wal_dir(args, id), false)?);
        total_bytes += store.total().apparent() + wal.apparent();
        partitions.push(format!(
            concat!(
                "{{\"partition\":{},\"commits\":{},\"refused\":{},\"delivered\":{},\"relations_per_s\":{:.0},",
                "\"syncs_per_s\":{:.1},\"store\":{},\"wal\":{}}}"
            ),
            id.get(),
            s.committed,
            s.refused,
            s.delivered,
            relations(s) as f64 / secs,
            s.rounds as f64 / secs,
            size_json(store.total()),
            size_json(wal),
        ));
    }
    let registrations: u64 = stats.iter().map(|s| s.registrations).sum();
    let mut waits: Vec<u64> = stats
        .iter()
        .flat_map(|s| s.waits_us.iter().copied())
        .collect();
    let wait = Summary::of(&mut waits);
    let total_relations: u64 = stats.iter().map(relations).sum();
    let rounds: u64 = stats.iter().map(|s| s.rounds).sum();
    Ok(format!(
        concat!(
            "{{\"run\":\"partitions\",\"partitions\":{},\"seed\":{},\"cross_percent\":{},\"last_close\":{},\"seconds\":{:.2},",
            "\"relations_per_s\":{:.0},\"syncs_per_s\":{:.1},\"disk_bytes\":{},\"registrations\":{},",
            "\"registrations_per_s\":{:.1},",
            "\"first_reference_wait_us\":{{\"count\":{},\"p50\":{},\"p99\":{},\"max\":{},\"mean\":{:.1}}},",
            "\"verified_reverse_reads\":{},\"per_partition\":[{}]}}"
        ),
        args.partitions(),
        args.seed(),
        args.cross_percent(),
        last_close.get(),
        secs,
        total_relations as f64 / secs,
        rounds as f64 / secs,
        total_bytes,
        registrations,
        registrations as f64 / secs,
        wait.count(),
        wait.p50(),
        wait.p99(),
        wait.max(),
        wait.mean(),
        verified.map_or_else(|| "null".to_string(), |v| v.to_string()),
        partitions.join(","),
    ))
}
