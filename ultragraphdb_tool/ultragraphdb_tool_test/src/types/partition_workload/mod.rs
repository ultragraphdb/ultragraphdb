/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Partitions on the simulator: each a [`PartitionHost`] over a simulated store and log of its own, exchanging
//! registrations as messages on one thread, in an order the seed draws. Writes put edges between the partitions and
//! remove them, closes reach each partition in its own time, and faults fail batches and reads, crash a partition and cut
//! its power; a partition that stops recovers from its log while the others go on, and writes waiting on it send their
//! registrations again.
//!
//! After every step the invariants of group 6b's section 6b.4 hold, read with no read failing, since the check is the
//! oracle: every commit stamped one above a close delivered to it (1), every committed reference covered by a referrer-set
//! member added at or below its epoch (2, 5), a member's epoch never changing (4), every reverse read at a closed epoch
//! answering what a reference model of one partition answers, at the latest closed epoch and an earlier one (6), and a
//! mark resting on a member at or above its generation (8). Catalog activation (3) has no version here, the sequencer's
//! and the metadata group's scenarios holding it; a stale report (7) waits for reclamation in stage 8.

mod edge_model;
mod partition_counts;

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::num::NonZeroU64;
use std::pin::pin;
use std::task::{Context as TaskContext, Poll, Waker};

use oxrdf::{GraphName, NamedNode};
use ultragraphdb_cluster_partition::{
    Fanout, Partition, PartitionError, PartitionErrorEnum, PartitionHost, PartitionMailbox,
    PartitionMessage, PartitionOutcome, ReverseQuery,
};
use ultragraphdb_cluster_placement::{NodeId, PartitionId, PlacementMap};
use ultragraphdb_cluster_sequencer::Close;
use ultragraphdb_core_ids::{BucketGroup, Epoch, SystemId, Version};
use ultragraphdb_core_keys::{Graph, QuadKey};
use ultragraphdb_core_mapping::{Change, Changeset, RandomSource};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_apply::{ChangesetApplier, ChangesetError};
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_quads::{QuadStore, iri_bucket};
use ultragraphdb_store_sim::{
    FaultConfig, Seed, SimClock, SimListCache, SimRng, SimStore, SteppedExecutor, Stream,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_traits_blocks::TestBlockSource;
use ultragraphdb_transactions_commit::{
    CloseRequest, CommitErrorEnum, Pipeline, Replay, RoundBounds, Sessions,
};
use ultragraphdb_transactions_log::{SimStorage, Wal, WalConfig};
use ultragraphdb_transactions_txn::{TxnError, TxnErrorEnum};

use crate::types::changeset_workload::injected;
use crate::types::partition_workload::edge_model::{Edge, EdgeModel};
pub use crate::types::partition_workload::partition_counts::PartitionCounts;

type Part = Partition<SimStore, SimListCache<CachedList>, SteppedExecutor, TestBlockSource>;
type Host = PartitionHost<
    'static,
    Wal<SimStorage>,
    SimStore,
    SimListCache<CachedList>,
    SteppedExecutor,
    TestBlockSource,
>;

/// The namespace the workload writes.
const NS: u64 = 0;
/// The cluster its logs belong to.
const CLUSTER: u64 = 1;
/// How long a step takes on the hosts' clock: a registration unanswered for 100 steps goes again.
const STEP_US: u64 = 10_000;
/// The subjects and objects each partition homes.
const NAMES_PER_PARTITION: usize = 4;
/// The tickets a host's restart skips: more than a run gives.
const TICKETS_PER_START: u64 = 1 << 32;
/// How many polls a simulated log's future takes at most.
const POLLS_MAX: u32 = 1_000_000;
/// How many recoveries in a row one crash may take before the run stops, each interrupted by a fault of its own.
const RECOVERIES_MAX: usize = 64;

/// A write under way: the edge it changes, whether it inserts it, and the latest close delivered when it was taken.
#[derive(Debug, Clone)]
struct Pending {
    edge: Edge,
    insert: bool,
    delivered: Epoch,
}

/// One partition and what outlives its crashes.
struct Node {
    id: PartitionId,
    part: &'static Part,
    host: Option<Host>,
    store: SimStore,
    storage: SimStorage,
    blocks: TestBlockSource,
    starts: u64,
    pending: BTreeMap<u64, Pending>,
    last_version: u64,
    /// The next close to deliver.
    next_close: Epoch,
}

/// See the module's documentation. A run is a function of its seed.
pub struct PartitionWorkload {
    seed: u64,
    faults: FaultConfig,
    rng: SimRng,
    placement: PlacementMap,
    nodes: Vec<Node>,
    senders: Vec<std::sync::mpsc::Sender<PartitionMessage>>,
    model: EdgeModel,
    /// The names of each partition's subjects and of every object.
    subjects: Vec<Vec<String>>,
    objects: Vec<String>,
    predicates: Vec<String>,
    /// Each member's epoch when first seen: it never changes.
    member_epochs: BTreeMap<(String, u32), Epoch>,
    counts: PartitionCounts,
    step: u64,
}

impl PartitionWorkload {
    /// Runs `steps` steps of `seed` over `partitions` partitions under `faults`: the workload as it ended.
    ///
    /// # Errors
    ///
    /// The first invariant broken, naming the seed and the step, or an error no fault explains.
    pub fn run(seed: u64, partitions: u32, faults: FaultConfig, steps: u64) -> TestResult<Self> {
        let mut workload = Self::new(seed, partitions, faults)?;
        for step in 1..=steps {
            workload.step = step;
            workload.one_step()?;
        }
        Ok(workload)
    }

    /// The counts of the run.
    pub fn counts(&self) -> PartitionCounts {
        self.counts
    }

    fn new(seed: u64, partitions: u32, faults: FaultConfig) -> TestResult<Self> {
        let placement = PlacementMap::even(partitions, NodeId::new(1))?;
        let ids = (1..=partitions)
            .map(PartitionId::new)
            .collect::<Result<Vec<_>, _>>()?;
        let mesh = PartitionMailbox::mesh(&ids);
        let senders = mesh
            .iter()
            .map(|m| m.sender(m.id()).context("a sender to itself"))
            .collect::<TestResult<Vec<_>>>()?;
        let first = Epoch::new(1)?;
        let mut nodes = Vec::new();
        for (id, mailbox) in ids.iter().copied().zip(mesh) {
            let node_seed = seed.wrapping_mul(31).wrapping_add(u64::from(id.get()));
            let store = SimStore::new(Seed::new(node_seed), faults, SimClock::new());
            let storage = SimStorage::new(node_seed);
            let blocks = TestBlockSource::new(NonZeroU64::MIN.saturating_add(99));
            let mut node = Node {
                id,
                part: open_part(id, &placement, &store, &blocks, 0)?,
                host: None,
                store,
                storage,
                blocks,
                starts: 1,
                pending: BTreeMap::new(),
                last_version: 0,
                next_close: first.next()?,
            };
            let (log, _) = block_on(Wal::open(
                node.storage.clone(),
                "/wal",
                WalConfig::new(CLUSTER, false),
            ))??;
            node.host = Some(host_of(&node, log, mailbox, first)?);
            nodes.push(node);
        }
        let mut names = Names::new(&placement, first);
        let subjects = ids
            .iter()
            .map(|id| names.of(*id, "s", NAMES_PER_PARTITION))
            .collect::<TestResult<Vec<_>>>()?;
        let mut objects = Vec::new();
        for id in &ids {
            objects.extend(names.of(*id, "o", NAMES_PER_PARTITION)?);
        }
        Ok(Self {
            seed,
            faults,
            rng: Seed::new(seed).stream(Stream::Workload),
            placement,
            nodes,
            senders,
            model: EdgeModel::default(),
            subjects,
            objects,
            predicates: vec!["knows".to_string(), "likes".to_string()],
            member_epochs: BTreeMap::new(),
            counts: PartitionCounts::default(),
            step: 0,
        })
    }

    /// One step: what the seed draws, every host's turn in a drawn order, recovery of what stopped, then the checks.
    fn one_step(&mut self) -> TestResult {
        self.act()?;
        let mut order: Vec<usize> = (0..self.nodes.len()).collect();
        for at in (1..order.len()).rev() {
            order.swap(at, self.rng.index(at + 1));
        }
        for at in order {
            self.turn(at)?;
        }
        for at in 0..self.nodes.len() {
            let node = self.nodes.get(at).context("the node")?;
            let stopped = node.host.as_ref().is_none_or(Host::is_stopped);
            if stopped || node.store.needs_recovery() {
                self.recover(at)?;
            }
        }
        self.check()
    }

    /// Draws what the step does: a write, a close, a fault between steps, or nothing.
    fn act(&mut self) -> TestResult {
        match self.rng.below(100) {
            0..55 => self.write()?,
            55..75 => {
                let at = self.rng.index(self.nodes.len());
                self.deliver_close(at)?;
            }
            75..80 => {
                for at in 0..self.nodes.len() {
                    self.deliver_close(at)?;
                }
            }
            _ => {}
        }
        let at = self.rng.index(self.nodes.len());
        let node = self.nodes.get(at).context("the node")?;
        if self.rng.chance(self.faults.crash_between_steps()) {
            node.store.crash();
        } else if self.rng.chance(self.faults.power_loss_between_steps()) {
            node.store.power_loss();
            node.storage.power_loss();
        }
        Ok(())
    }

    /// Submits a change of an edge from a subject of a drawn partition to any object.
    fn write(&mut self) -> TestResult {
        let at = self.rng.index(self.nodes.len());
        let of_partition = self.subjects.get(at).context("the partition's subjects")?;
        let subject = of_partition
            .get(self.rng.index(of_partition.len()))
            .context("a subject")?
            .clone();
        let predicate = self
            .predicates
            .get(self.rng.index(self.predicates.len()))
            .context("a predicate")?
            .clone();
        let object = self
            .objects
            .get(self.rng.index(self.objects.len()))
            .context("an object")?
            .clone();
        let edge: Edge = (subject, predicate, object);
        // A held edge is removed half the time, so reads see both directions of its history.
        let insert = !(self.model.held_at(&edge, u64::MAX) && self.rng.chance(500));
        let change = edge_change(&edge, insert);
        let now = self.now();
        let node = self.nodes.get_mut(at).context("the node")?;
        let Some(host) = node.host.as_mut() else {
            return Ok(());
        };
        let delivered = host.delivered();
        if let Err(stopped) = host.submit(Changeset::new(NS).with(change), now) {
            return explained(&stopped)
                .map_err(|detail| self.failure("no unexpected error", detail));
        }
        let ticket = host.last_ticket();
        let _previous = node.pending.insert(
            ticket,
            Pending {
                edge,
                insert,
                delivered,
            },
        );
        Ok(())
    }

    /// Delivers the partition at `at` its next close.
    fn deliver_close(&mut self, at: usize) -> TestResult {
        let node = self.nodes.get_mut(at).context("the node")?;
        let close = Close::new(node.next_close, 0, Vec::new(), Vec::new())?;
        node.next_close = node.next_close.next()?;
        self.senders
            .get(at)
            .context("the sender")?
            .send(PartitionMessage::Close(close))?;
        Ok(())
    }

    /// The host at `at`'s turn, its outcomes taken into the model.
    fn turn(&mut self, at: usize) -> TestResult {
        let now = self.now();
        let node = self.nodes.get_mut(at).context("the node")?;
        let Some(host) = node.host.as_mut() else {
            return Ok(());
        };
        let turned = block_on(host.turn(now))?;
        let outcomes = host.take_outcomes();
        if let Err(stopped) = turned {
            explained(&stopped).map_err(|detail| self.failure("no unexpected error", detail))?;
        }
        for outcome in outcomes {
            self.ended(at, &outcome)?;
        }
        Ok(())
    }

    /// Takes the outcome of a write into the model, checking its stamp (invariant 1).
    fn ended(&mut self, at: usize, outcome: &PartitionOutcome) -> TestResult {
        let node = self.nodes.get_mut(at).context("the node")?;
        let pending = node.pending.remove(&outcome.ticket());
        let delivered = node.host.as_ref().map(Host::delivered);
        let last = node.last_version;
        let version = match outcome.outcome() {
            Ok(version) => *version,
            Err(refusal) => {
                self.counts.refusals += 1;
                return explained(refusal)
                    .map_err(|detail| self.failure("no unexpected error", detail));
            }
        };
        node.last_version = version;
        let pending = pending.context("a commit answers a write the host took")?;
        let stamped = Version::from_u64(version).epoch();
        let above_submit = pending.delivered.get() + 1;
        let above_latest = delivered.context("a host")?.get() + 1;
        if stamped.get() < above_submit || stamped.get() > above_latest {
            return Err(self.failure(
                "stamps_above_delivered_close",
                format!("epoch {stamped:?} outside {above_submit}..={above_latest}"),
            ));
        }
        if version <= last {
            return Err(self.failure(
                "stamps_never_fall_along_a_key",
                format!("version {version} after {last}"),
            ));
        }
        self.counts.commits += 1;
        self.counts.registrations += outcome.registrations() as u64;
        self.model.record(pending.edge, version, pending.insert);
        Ok(())
    }

    /// Recovers the partition at `at`: its store reopened, its log replayed into it, a new host, and what the crash left
    /// in doubt read from the store.
    fn recover(&mut self, at: usize) -> TestResult {
        self.counts.recoveries += 1;
        let placement = self.placement.clone();
        let node = self.nodes.get_mut(at).context("the node")?;
        let host = node.host.take().context("a host")?;
        let delivered = host.delivered();
        let (mailbox, lost) = host.into_parts();
        drop(lost);
        let mut attempts = 0;
        // Ends: a recovery interrupted by a batch's fault starts again, at most `RECOVERIES_MAX` times.
        let (part, log) = loop {
            node.store.recover();
            attempts += 1;
            match reopen(node, &placement) {
                Ok(opened) => break opened,
                Err(refused) if attempts >= RECOVERIES_MAX => return Err(refused),
                Err(_) => {}
            }
        };
        node.part = part;
        node.starts += 1;
        node.host = Some(host_of(node, log, mailbox, delivered)?);
        node.last_version = part.applier().quads().lists().applied_ts();
        let in_doubt: Vec<Pending> = std::mem::take(&mut node.pending).into_values().collect();
        for pending in in_doubt {
            self.settle_in_doubt(at, &pending)?;
        }
        Ok(())
    }

    /// Takes a write whose reply a crash lost into the model, as the store holds it: the round that held it was durable
    /// or it was not. Every version of the subject's list where the edge's state changes is a change the model records.
    fn settle_in_doubt(&mut self, at: usize, pending: &Pending) -> TestResult {
        let (subject, predicate, object) = &pending.edge;
        let part = self.nodes.get(at).context("the node")?.part;
        let store = part.applier().quads().store().clone();
        let quads = part.applier().quads();
        let ids = store.unfaulted_reads(|| -> TestResult<_> {
            Ok((
                quads.id_of(NS, ex(subject).as_str())?,
                quads.id_of(NS, ex(predicate).as_str())?,
                quads.id_of(NS, ex(object).as_str())?,
            ))
        })?;
        let (Some(s), Some(p), Some(o)) = ids else {
            return Ok(());
        };
        let key = QuadKey::Forward {
            namespace: NS,
            graph: Graph::Default,
            predicate: p,
            subject: s,
        }
        .to_bytes();
        let contents = store.contents();
        let mut versions: Vec<u64> = contents
            .versions(&key)
            .iter()
            .map(|v| v.version())
            .collect();
        versions.sort_unstable();
        let target = EncodedTerm::id(o);
        for version in versions {
            let objects = store.unfaulted_reads(|| quads.objects(&key, version))?;
            let held = objects.iter().any(|(term, _)| *term == target);
            if self.model.held_at(&pending.edge, version) != held {
                self.model.record(pending.edge.clone(), version, held);
                self.counts.in_doubt_applied += 1;
            }
        }
        Ok(())
    }

    /// The invariants, read with no read failing.
    fn check(&mut self) -> TestResult {
        let stores: Vec<SimStore> = self.nodes.iter().map(|n| n.store.clone()).collect();
        unfaulted(&stores, || self.check_held())?;
        let closed = self
            .nodes
            .iter()
            .filter_map(|n| n.host.as_ref().map(Host::delivered))
            .min()
            .context("a partition")?;
        let earlier = Epoch::new(self.rng.between(1, closed.get().max(1)))?;
        for at in [closed, earlier] {
            unfaulted(&stores, || self.check_reverse_reads(at))?;
        }
        Ok(())
    }

    /// Invariants 2, 4, 5 and 8 over every edge held: its object's referrer set names its subject's group, added at or
    /// below the edge's epoch, at the epoch it was first seen with, and a subject's partition other than the object's home
    /// holds a mark no newer than the member.
    fn check_held(&mut self) -> TestResult {
        let held: Vec<(Edge, u64)> = self.model.held().map(|(e, v)| (e.clone(), v)).collect();
        for ((subject, _, object), version) in held {
            let group = BucketGroup::of(bucket_of(&subject)?);
            let home = self.home_of(&object)?;
            let id = home
                .applier()
                .quads()
                .id_of(NS, ex(&object).as_str())?
                .ok_or_else(|| {
                    self.failure(
                        "epoch_inequality",
                        format!("{object} has no ID at its home"),
                    )
                })?;
            let set = home.applier().quads().referrers(NS, id, u64::MAX)?;
            let member = set.member(group).ok_or_else(|| {
                self.failure(
                    "reclamation_keeps_every_member_with_an_edge",
                    format!("R({object}) lacks {subject}'s group"),
                )
            })?;
            let at = Version::from_u64(version).epoch();
            if member.epoch() > at {
                return Err(self.failure(
                    "epoch_inequality",
                    format!(
                        "R({object}) added {subject}'s group at {:?}, the edge at {at:?}",
                        member.epoch()
                    ),
                ));
            }
            let first = *self
                .member_epochs
                .entry((object.clone(), group.get()))
                .or_insert(member.epoch());
            if first != member.epoch() {
                return Err(self.failure(
                    "stamps_never_fall_along_a_key",
                    format!(
                        "R({object})'s member moved from {first:?} to {:?}",
                        member.epoch()
                    ),
                ));
            }
            let generation = member.generation();
            self.check_mark(&subject, id, group, generation, home.id())?;
        }
        Ok(())
    }

    /// The mark of `(entity, group)` at `subject`'s partition, when that is not the home: held, and no newer than the
    /// member's `generation`.
    fn check_mark(
        &self,
        subject: &str,
        entity: SystemId,
        group: BucketGroup,
        generation: NonZeroU64,
        home: PartitionId,
    ) -> TestResult {
        let holder = self.home_of(subject)?;
        if holder.id() == home {
            return Ok(());
        }
        let mark = holder.applier().quads().mark(NS, entity, group, u64::MAX)?;
        match mark {
            Some(marked) if marked <= generation => Ok(()),
            other => Err(self.failure(
                "mark rests on its member",
                format!("{subject}'s mark {other:?} over generation {generation}"),
            )),
        }
    }

    /// Invariant 6: every reverse read at `at`, a closed epoch, answers the reference model.
    fn check_reverse_reads(&mut self, at: Epoch) -> TestResult {
        let top = ((at.get() + 1) << 22) - 1;
        for object in self.objects.clone() {
            for predicate in self.predicates.clone() {
                let expected = self.model.subjects_at(&predicate, &object, top);
                let found = self.reverse_read(&predicate, &object, at)?;
                self.counts.reverse_reads += 1;
                if !found.is_empty() {
                    self.counts.reverse_rows += 1;
                }
                if found != expected {
                    return Err(self.failure(
                        "reverse_read_complete",
                        format!(
                            "{predicate} of {object} at {at:?}: {found:?}, the model {expected:?}"
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    /// The names of the subjects a reverse read of `object` through `predicate` at `at` returns.
    fn reverse_read(
        &self,
        predicate: &str,
        object: &str,
        at: Epoch,
    ) -> TestResult<BTreeSet<String>> {
        let home = self.home_of(object)?;
        let held = (
            home.applier().quads().id_of(NS, ex(object).as_str())?,
            self.home_of(predicate)?
                .applier()
                .quads()
                .id_of(NS, ex(predicate).as_str())?,
        );
        let (Some(o), Some(p)) = held else {
            return Ok(BTreeSet::new());
        };
        let query = ReverseQuery::new(home.id(), NS, Graph::Default, p, o, at);
        let partitions: Vec<&Part> = self.nodes.iter().map(|n| n.part).collect();
        let mut subjects = Vec::new();
        let _asked = Fanout::over(partitions).subjects(&query, &mut subjects)?;
        subjects
            .into_iter()
            .map(|id| {
                let holder = self.part_of(id)?;
                let (iri, _) = holder.applier().quads().iri_of(NS, id)?;
                iri.strip_prefix("http://ex.org/")
                    .map(ToString::to_string)
                    .context("a workload subject")
            })
            .collect()
    }

    /// The partition owning `<name>`'s bucket.
    fn home_of(&self, name: &str) -> TestResult<&'static Part> {
        let bucket = bucket_of(name)?;
        let owner = self.placement.partition_of(bucket, Epoch::new(1)?);
        self.part(owner)
    }

    /// The partition owning `id`'s bucket.
    fn part_of(&self, id: SystemId) -> TestResult<&'static Part> {
        self.part(self.placement.partition_of(id.bucket(), Epoch::new(1)?))
    }

    fn part(&self, id: PartitionId) -> TestResult<&'static Part> {
        self.nodes
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.part)
            .context("a partition of the map")
    }

    fn now(&self) -> u64 {
        self.step.saturating_mul(STEP_US)
    }

    fn failure(&self, rule: &str, detail: impl std::fmt::Display) -> TestError {
        TestError::new(format!(
            "seed {} step {}: {rule}: {detail}",
            self.seed, self.step
        ))
    }
}

/// Picks names of partitions by the placement map.
struct Names<'p> {
    placement: &'p PlacementMap,
    at: Epoch,
    next: u32,
}

impl<'p> Names<'p> {
    fn new(placement: &'p PlacementMap, at: Epoch) -> Self {
        Self {
            placement,
            at,
            next: 0,
        }
    }

    /// `count` names `prefix<n>` homed in partition `id`.
    fn of(&mut self, id: PartitionId, prefix: &str, count: usize) -> TestResult<Vec<String>> {
        let mut names = Vec::with_capacity(count);
        // Ends: a hash places about one name in `partitions` in each, and the search is bounded.
        for _ in 0..1_000_000 {
            if names.len() == count {
                return Ok(names);
            }
            self.next += 1;
            let name = format!("{prefix}{}", self.next);
            if self.placement.partition_of(bucket_of(&name)?, self.at) == id {
                names.push(name);
            }
        }
        Err(TestError::new("no names of the partition"))
    }
}

/// The IRI `http://ex.org/<name>`.
fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

/// The bucket of `<name>`.
fn bucket_of(name: &str) -> TestResult<ultragraphdb_core_ids::Bucket> {
    iri_bucket(NS, ex(name).as_str()).context("a bucket")
}

/// The change of `edge`.
fn edge_change((subject, predicate, object): &Edge, insert: bool) -> Change {
    let (graph, subject, predicate, object) = (
        GraphName::DefaultGraph,
        ex(subject).into(),
        ex(predicate),
        ex(object).into(),
    );
    if insert {
        Change::Insert {
            graph,
            subject,
            predicate,
            object,
        }
    } else {
        Change::Delete {
            graph,
            subject,
            predicate,
            object,
        }
    }
}

/// A partition over `store`, owning its buckets, as a process opening it after `starts` starts does.
fn open_part(
    id: PartitionId,
    placement: &PlacementMap,
    store: &SimStore,
    blocks: &TestBlockSource,
    starts: u64,
) -> TestResult<&'static Part> {
    let value = u64::from(id.get()).wrapping_add(starts.wrapping_mul(1_000));
    let seed = Seed::new(value);
    let cache = SimListCache::new(1024, seed.stream(Stream::Cache));
    let owned = Part::owned(id, placement, Epoch::new(1)?).context("the map's ranges")?;
    let quads = QuadStore::open(
        Lists::new(store.clone(), cache, ListsConfig::default()),
        blocks.clone(),
        RandomSource::new(value),
    )?
    .owning(owned);
    let applier = ChangesetApplier::over(
        quads,
        SteppedExecutor::new(seed.stream(Stream::Executor)),
        1,
    )?;
    // Leaked: hosts hold their partition for the run, and a partition recovers beside hosts of the others. A test
    // process's few recoveries end with it.
    Ok(Box::leak(Box::new(Partition::new(
        id,
        placement.clone(),
        applier,
    ))))
}

/// `node`'s host over `log`, the latest close delivered to it `delivered`.
fn host_of(
    node: &Node,
    log: Wal<SimStorage>,
    mailbox: PartitionMailbox,
    delivered: Epoch,
) -> TestResult<Host> {
    let part = node.part;
    let pipeline = Pipeline::new(part.applier(), log, RoundBounds::new(u32::MAX, u32::MAX))?
        .with_partition(part.id().get(), delivered, CloseRequest::default(), &[])
        .bound(part.id(), part.placement());
    Ok(PartitionHost::new(part, pipeline, mailbox)
        .with_first_ticket(node.starts.saturating_mul(TICKETS_PER_START)))
}

/// `node`'s store and log opened again and the log replayed, a batch's injected fault refusing it.
fn reopen(node: &Node, placement: &PlacementMap) -> TestResult<(&'static Part, Wal<SimStorage>)> {
    let part = open_part(node.id, placement, &node.store, &node.blocks, node.starts)?;
    let (mut log, _) = block_on(Wal::open(
        node.storage.clone(),
        "/wal",
        WalConfig::new(CLUSTER, false),
    ))??;
    let replayed = node.store.unfaulted_reads(|| {
        block_on(Replay::run(
            part.applier(),
            &mut log,
            Sessions::DEFAULT_CAPACITY,
        ))
    })?;
    let _replay = replayed.map_err(|e| TestError::new(e.to_string()))?;
    Ok((part, log))
}

/// Runs `f` with no read of any of `stores` failing.
fn unfaulted<T>(stores: &[SimStore], f: impl FnOnce() -> T) -> T {
    match stores.split_first() {
        Some((store, rest)) => store.unfaulted_reads(|| unfaulted(rest, f)),
        None => f(),
    }
}

/// Runs `future` to its end: the simulated log's futures wait on nothing.
fn block_on<F: Future>(future: F) -> TestResult<F::Output> {
    let mut future = pin!(future);
    let mut context = TaskContext::from_waker(Waker::noop());
    for _ in 0..POLLS_MAX {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return Ok(output);
        }
    }
    Err(TestError::new("a simulated log's future stayed pending"))
}

/// Whether a fault or a concurrent write explains `error`; otherwise its text.
fn explained(error: &PartitionError) -> Result<(), String> {
    let fine = match &error.0 {
        PartitionErrorEnum::Txn(TxnError(TxnErrorEnum::Changeset(e))) => injected(e),
        PartitionErrorEnum::Txn(TxnError(TxnErrorEnum::PastLimit { .. })) => true,
        PartitionErrorEnum::Changeset(e) => injected(e),
        PartitionErrorEnum::Quad(e) => injected(&ChangesetError::from(e.clone())),
        PartitionErrorEnum::Commit(e) => matches!(
            e.0,
            CommitErrorEnum::Conflict { .. }
                | CommitErrorEnum::TooOld { .. }
                | CommitErrorEnum::Busy { .. }
                | CommitErrorEnum::Unavailable
        ),
        PartitionErrorEnum::Pipeline(_) => true,
        PartitionErrorEnum::Txn(_)
        | PartitionErrorEnum::WrongPartition { .. }
        | PartitionErrorEnum::CrossPartition { .. }
        | PartitionErrorEnum::TimedOut { .. }
        | PartitionErrorEnum::Refused { .. } => false,
    };
    if fine { Ok(()) } else { Err(error.to_string()) }
}
