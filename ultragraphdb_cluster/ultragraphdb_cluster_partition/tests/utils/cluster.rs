/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the unit tests and the scenarios share: a cluster of partitions in one process, each a [`Partition`] over its
//! own fault-free simulated store with its own write-ahead log and pipeline, and a home's state; and the full write
//! path between them — a transaction begun at its partition, its references registered at their homes and answered
//! once durable, adopted, written and committed — each operation run to its end, every round synced and applied.

use std::future::Future;
use std::num::NonZeroU64;
use std::pin::pin;
use std::task::{Context as TaskContext, Poll, Waker};

use oxrdf::{GraphName, Literal, NamedNode};
use ultragraphdb_cluster_partition::{Answer, Home, Partition, PartitionError, Reference};
use ultragraphdb_cluster_placement::{NodeId, PartitionId, PlacementMap};
use ultragraphdb_cluster_sequencer::Close;
use ultragraphdb_core_ids::{
    Bucket, BucketGroup, CanonicalKey, Epoch, SystemId, Tenant, TypePrefix,
};
use ultragraphdb_core_mapping::{
    Change, Changeset, Declaration, PredicateDeclaration, RandomSource,
};
use ultragraphdb_store_apply::ChangesetApplier;
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_quads::QuadStore;
use ultragraphdb_store_sim::{
    FaultConfig, Seed, SimClock, SimListCache, SimStore, SteppedExecutor, Stream,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_traits_blocks::TestBlockSource;
use ultragraphdb_transactions_commit::{CloseRequest, CommitError, Pipeline, Reply, RoundBounds};
use ultragraphdb_transactions_log::{SimStorage, Wal, WalConfig};
use ultragraphdb_transactions_txn::{
    Commit, Mode, Prepared, Transaction, TxnBuffers, TxnError, TxnLimits,
};

/// A partition of the tests: over a simulated store, its cache and its stepped executor.
pub type Part = Partition<SimStore, SimListCache<CachedList>, SteppedExecutor, TestBlockSource>;
/// A transaction on a partition of the tests.
pub type Txn<'a> =
    Transaction<'a, SimStore, SimListCache<CachedList>, SteppedExecutor, TestBlockSource>;
/// The write-ahead log of the tests, over simulated storage.
pub type Log = Wal<SimStorage>;
/// A partition's pipeline.
pub type Pipe<'a> =
    Pipeline<'a, Log, SimStore, SimListCache<CachedList>, SteppedExecutor, TestBlockSource>;

/// How many polls a simulated log's future takes at most: it waits on nothing but its own steps.
const POLLS_MAX: u32 = 1_000_000;

/// Runs `future` to its end, refused past [`POLLS_MAX`] polls: a scenario's step already runs on the cucumber runner's
/// executor, which another executor cannot enter.
pub fn block_on<F: Future>(future: F) -> TestResult<F::Output> {
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

/// The tenant the tests write.
pub const TENANT: u64 = 0;

/// The IRI `http://ex.org/<name>`.
pub fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

/// The bucket of `<name>` in [`TENANT`].
pub fn bucket(name: &str) -> TestResult<Bucket> {
    Ok(Bucket::derive(
        Tenant::new(TENANT),
        TypePrefix::RESOURCE,
        &CanonicalKey::iri(ex(name).as_str())?,
    ))
}

/// The bucket group of `<name>`'s bucket.
pub fn group(name: &str) -> TestResult<BucketGroup> {
    Ok(BucketGroup::of(bucket(name)?))
}

/// The partition `id`.
pub fn pid(id: u32) -> TestResult<PartitionId> {
    Ok(PartitionId::new(id)?)
}

/// The highest version a read as of `epoch` sees: every commit stamped at or below it.
pub fn as_of(epoch: Epoch) -> u64 {
    (epoch.get() << 22) | ((1 << 22) - 1)
}

/// The epoch `epoch`.
pub fn epoch(epoch: u64) -> TestResult<Epoch> {
    Ok(Epoch::new(epoch)?)
}

/// The insert of `<subject> <predicate> <object>`, all three IRIs under `http://ex.org/`.
pub fn insert(subject: &str, predicate: &str, object: &str) -> Change {
    Change::Insert {
        graph: GraphName::DefaultGraph,
        subject: ex(subject).into(),
        predicate: ex(predicate),
        object: ex(object).into(),
    }
}

/// The insert of `<subject> <predicate> "value"`.
pub fn insert_literal(subject: &str, predicate: &str, value: &str) -> Change {
    Change::Insert {
        graph: GraphName::DefaultGraph,
        subject: ex(subject).into(),
        predicate: ex(predicate),
        object: Literal::new_simple_literal(value).into(),
    }
}

/// A changeset of [`TENANT`] holding `changes`.
pub fn changeset(changes: Vec<Change>) -> Changeset {
    changes
        .into_iter()
        .fold(Changeset::new(TENANT), |cs, change| cs.with(change))
}

/// How a write through the cluster ended: committed at its version, refused by the partition before its commit (the
/// transaction's own refusal included), or refused by the commit step.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Committed(u64),
    Refused(PartitionError),
    Rejected(CommitError),
}

/// What a write through the cluster did: the references its transaction registered, each with its home's answer, and
/// how it ended.
#[derive(Debug)]
pub struct Written {
    pub references: Vec<(Reference, Answer)>,
    pub outcome: Outcome,
}

impl Written {
    /// The commit's version, or the write's refusal as a test error.
    pub fn version(&self) -> TestResult<u64> {
        match &self.outcome {
            Outcome::Committed(version) => Ok(*version),
            refusal @ (Outcome::Refused(_) | Outcome::Rejected(_)) => Err(TestError::new(format!(
                "the write was refused: {refusal:?}"
            ))),
        }
    }
}

/// A cluster of partitions in one process: by position, partition `i + 1` and its log, the latest close delivered to
/// it and its home's state.
pub struct Cluster {
    parts: Vec<Part>,
    storages: Vec<SimStorage>,
    logs: Vec<Option<Log>>,
    delivered: Vec<Epoch>,
    homes: Vec<Home>,
    ticket: u64,
    /// The highest epoch closed so far, delivered to every partition.
    closed: Epoch,
}

impl Cluster {
    /// `count` partitions over the even placement map, the first close, epoch 1, delivered to each.
    pub fn even(count: u32) -> TestResult<Self> {
        Self::with_placement(&PlacementMap::even(count, NodeId::new(1))?)
    }

    /// The partitions of `placement`, epoch 1 closed, each having written its tenant's catalog, so the first writes
    /// of a test register what they name and no more.
    pub fn with_placement(placement: &PlacementMap) -> TestResult<Self> {
        let mut cluster = Self::bare(placement)?;
        for id in 1..=u32::try_from(cluster.parts.len())? {
            let _written = cluster.write(id, Vec::new())?.version()?;
        }
        Ok(cluster)
    }

    /// The partitions of `placement`, every partition it names one, epoch 1 closed, none having written.
    pub fn bare(placement: &PlacementMap) -> TestResult<Self> {
        let first = epoch(1)?;
        let mut ids: Vec<PartitionId> = placement
            .ranges_at(first)
            .into_iter()
            .map(|(_, p)| p)
            .collect();
        ids.sort();
        ids.dedup();
        let mut cluster = Self {
            parts: Vec::new(),
            storages: Vec::new(),
            logs: Vec::new(),
            delivered: Vec::new(),
            homes: Vec::new(),
            ticket: 0,
            closed: first,
        };
        for id in ids {
            let part = node(id, placement, first)?;
            let storage = SimStorage::new(u64::from(id.get()));
            let (log, _) = block_on(Wal::open(storage.clone(), "/wal", WalConfig::new(1, true)))?
                .context("the log opens")?;
            cluster.parts.push(part);
            cluster.storages.push(storage);
            cluster.logs.push(Some(log));
            cluster.delivered.push(first);
            cluster.homes.push(Home::new());
        }
        Ok(cluster)
    }

    /// The partitions, partition `i + 1` at position `i`.
    pub fn parts(&self) -> &[Part] {
        &self.parts
    }

    /// Partition `id`.
    pub fn part(&self, id: u32) -> TestResult<&Part> {
        self.parts
            .get(id as usize - 1)
            .context("the partition exists")
    }

    /// The latest close delivered to partition `id`.
    pub fn delivered(&self, id: u32) -> TestResult<Epoch> {
        self.delivered
            .get(id as usize - 1)
            .copied()
            .context("the partition exists")
    }

    /// The highest epoch closed so far.
    pub fn closed(&self) -> Epoch {
        self.closed
    }

    /// The name `prefix<n>` with the smallest `n` whose bucket partition `id` owns at the closed epoch.
    pub fn name_in(&self, id: u32, prefix: &str) -> TestResult<String> {
        self.name_where(prefix, |name| Ok(self.owner(name)? == id))
    }

    /// The name `prefix<n>` with the smallest `n` for which `wanted` holds.
    pub fn name_where(
        &self,
        prefix: &str,
        mut wanted: impl FnMut(&str) -> TestResult<bool>,
    ) -> TestResult<String> {
        for n in 0..100_000u32 {
            let name = format!("{prefix}{n}");
            if wanted(&name)? {
                return Ok(name);
            }
        }
        Err(TestError::new("no name of the wanted partition"))
    }

    /// The partition owning `<name>`'s bucket at the closed epoch.
    pub fn owner(&self, name: &str) -> TestResult<u32> {
        let placement = self.part(1)?.placement();
        Ok(placement.partition_of(bucket(name)?, self.closed).get())
    }

    /// Runs `f` on partition `id`'s pipeline, opened over its log with the latest close delivered to it and bound to
    /// it, and runs the pipeline to its end: every round synced and applied, its replies given to `f`'s caller.
    fn with_pipe<T>(
        &mut self,
        id: u32,
        f: impl FnOnce(&Part, &mut Pipe<'_>, &mut Home) -> TestResult<T>,
    ) -> TestResult<(T, Vec<Reply>)> {
        let at = id as usize - 1;
        let part = self.parts.get(at).context("the partition exists")?;
        let log = self
            .logs
            .get_mut(at)
            .and_then(Option::take)
            .context("the log is free")?;
        let delivered = *self.delivered.get(at).context("the partition exists")?;
        let home = self.homes.get_mut(at).context("the partition exists")?;
        let mut pipe = Pipeline::new(part.applier(), log, RoundBounds::new(u32::MAX, u32::MAX))?
            .with_partition(id, delivered, CloseRequest::default(), &[])
            .bound(part.id(), part.placement());
        let result = f(part, &mut pipe, home);
        let mut replies = Vec::new();
        for _ in 0..16 {
            if !pipe.needs_sync() {
                break;
            }
            block_on(pipe.sync(0))??;
            replies.append(pipe.replies_mut());
        }
        replies.append(pipe.replies_mut());
        let latest = pipe.delivered();
        if let Some(slot) = self.delivered.get_mut(at) {
            *slot = latest;
        }
        if let Some(slot) = self.logs.get_mut(at) {
            *slot = Some(pipe.into_log());
        }
        Ok((result?, replies))
    }

    /// Delivers the close of the epoch after the highest closed to every partition, at once: the sequencer's close
    /// reaching them all.
    pub fn close(&mut self) -> TestResult<Epoch> {
        let next = self.closed.next()?;
        for id in 1..=u32::try_from(self.parts.len())? {
            self.close_at(id, next)?;
        }
        self.closed = next;
        Ok(next)
    }

    /// Delivers the close of `epoch` to partition `id` alone.
    pub fn close_at(&mut self, id: u32, epoch: Epoch) -> TestResult {
        let close = Close::new(epoch, 0, Vec::new(), Vec::new())?;
        let ((), _) = self.with_pipe(id, |_, pipe, _| Ok(pipe.close(close, 0)?))?;
        if epoch > self.closed {
            self.closed = epoch;
        }
        Ok(())
    }

    /// Registers `reference` at its home, under a ticket of its own, and returns the home's answer once durable.
    pub fn register(&mut self, reference: &Reference) -> TestResult<Answer> {
        self.ticket += 1;
        let ticket = self.ticket;
        let (now, replies) = self.with_pipe(reference.home().get(), |part, pipe, home| {
            Ok(home.accept(part, pipe, ticket, reference, 0)?)
        })?;
        if let Some(answer) = now {
            return Ok(answer);
        }
        let at = reference.home().get() as usize - 1;
        let home = self.homes.get_mut(at).context("the home exists")?;
        let mut replies = replies;
        let mut settled = Vec::new();
        home.settle(&mut replies, &mut settled);
        let (_, answer) = settled
            .into_iter()
            .find(|(t, _)| *t == ticket)
            .context("the registration is answered once durable")?;
        Ok(answer?)
    }

    /// Accepts every one of `references` at partition `home` before any is durable, as registrants racing to the home
    /// do, and returns each answer, in order, once their entries are durable.
    pub fn register_all(&mut self, home: u32, references: &[Reference]) -> TestResult<Vec<Answer>> {
        let first = self.ticket + 1;
        self.ticket += references.len() as u64;
        let (now, mut replies) = self.with_pipe(home, |part, pipe, state| {
            references
                .iter()
                .zip(first..)
                .map(|(reference, ticket)| Ok(state.accept(part, pipe, ticket, reference, 0)?))
                .collect::<TestResult<Vec<Option<Answer>>>>()
        })?;
        let state = self
            .homes
            .get_mut(home as usize - 1)
            .context("the home exists")?;
        let mut settled = Vec::new();
        state.settle(&mut replies, &mut settled);
        now.into_iter()
            .zip(first..)
            .map(|(answer, ticket)| match answer {
                Some(answer) => Ok(answer),
                None => settled
                    .iter()
                    .find(|(t, _)| *t == ticket)
                    .context("the registration is answered once durable")?
                    .1
                    .clone()
                    .map_err(TestError::from),
            })
            .collect()
    }

    /// Declares `declaration`, of the predicate `<name>`, on every partition's applier outside its pipeline, as
    /// set-up, so each holds the same catalog: first at the predicate's home, then at each other partition once it
    /// holds the predicate's ID, got from its home as a first use gets it.
    pub fn declare(&mut self, name: &str, declaration: PredicateDeclaration) -> TestResult {
        let cs = changeset(vec![Change::Declare(Declaration::Predicate(declaration))]);
        let home = self.owner(name)?;
        let _applied = self.part(home)?.applier().apply(&cs)?;
        for id in 1..=u32::try_from(self.parts.len())? {
            if id == home {
                continue;
            }
            let reference = Reference::new(
                pid(home)?,
                TENANT,
                ex(name).as_str().to_string(),
                None,
                None,
                0,
            );
            let answer = self.register(&reference)?;
            let _copied = self
                .write_answered(id, Vec::new(), vec![(reference, answer)])?
                .version()?;
            let _declared = self.part(id)?.applier().apply(&cs)?;
        }
        Ok(())
    }

    /// The references `changes` written at partition `id` would register, sorted by home.
    pub fn references(&self, id: u32, changes: Vec<Change>) -> TestResult<Vec<Reference>> {
        let part = self.part(id)?;
        let mut txn = begin(part)?;
        let mut out = Vec::new();
        part.references(
            &mut txn,
            &changeset(changes),
            self.delivered(id)?.next()?,
            0,
            &mut out,
        )?;
        let _buffers: TxnBuffers = txn.abort();
        Ok(out)
    }

    /// Writes `changes` as a read-write transaction at partition `id`, bound to it: its references registered at their
    /// homes and adopted, then the changes written and committed.
    pub fn write(&mut self, id: u32, changes: Vec<Change>) -> TestResult<Written> {
        let references = self.references(id, changes.clone())?;
        let mut answered = Vec::new();
        for reference in references {
            let answer = self.register(&reference)?;
            answered.push((reference, answer));
        }
        self.write_answered(id, changes, answered)
    }

    /// Writes `changes` at partition `id` with `answered`, its references and their homes' answers, adopted first. The
    /// closes the answers name are delivered to the partition first when it has not received them, so the commit is
    /// stamped once its floor is open.
    pub fn write_answered(
        &mut self,
        id: u32,
        changes: Vec<Change>,
        answered: Vec<(Reference, Answer)>,
    ) -> TestResult<Written> {
        if let Some(floor) = answered.iter().map(|(_, a)| a.close()).max() {
            self.deliver_through(id, floor)?;
        }
        let prepared = match self.prepare(id, &answered, changes)? {
            Ok(prepared) => prepared,
            Err(refusal) => {
                return Ok(Written {
                    references: answered,
                    outcome: Outcome::Refused(refusal),
                });
            }
        };
        let outcome = self.commit(id, prepared)?;
        Ok(Written {
            references: answered,
            outcome,
        })
    }

    /// Delivers the closes after partition `id`'s latest through `epoch` to it alone.
    pub fn deliver_through(&mut self, id: u32, epoch: Epoch) -> TestResult {
        while self.delivered(id)? < epoch {
            let next = self.delivered(id)?.next()?;
            self.close_at(id, next)?;
        }
        Ok(())
    }

    /// A transaction at partition `id` that adopts `answered` and writes `changes`, ended as its commit's proposal, or
    /// the refusal that ended it.
    pub fn prepare(
        &self,
        id: u32,
        answered: &[(Reference, Answer)],
        changes: Vec<Change>,
    ) -> TestResult<Result<Prepared, PartitionError>> {
        let part = self.part(id)?;
        let mut txn = begin(part)?;
        let written = (|| -> Result<(), PartitionError> {
            for (reference, answer) in answered {
                part.adopt(&mut txn, reference, answer, 0)?;
            }
            Ok(txn.write(&changeset(changes), 0)?)
        })();
        if let Err(refusal) = written {
            return Ok(Err(refusal));
        }
        match txn.commit(0, 0, &[], 0) {
            Ok(Commit::ReadWrite(prepared)) => Ok(Ok(prepared)),
            Ok(Commit::ReadOnly(_)) => Err(TestError::new("a read-write commit")),
            Err(refusal) => Ok(Err(refusal.into())),
        }
    }

    /// Commits `prepared` through partition `id`'s pipeline: how its commit step answered it.
    pub fn commit(&mut self, id: u32, prepared: Prepared) -> TestResult<Outcome> {
        self.ticket += 1;
        let ticket = self.ticket;
        let ((), replies) =
            self.with_pipe(id, |_, pipe, _| Ok(pipe.commit(ticket, prepared, 0)?))?;
        let reply = replies
            .into_iter()
            .find(|r| r.ticket() == ticket)
            .context("the commit is answered")?;
        Ok(match reply.outcome() {
            Ok(version) => Outcome::Committed(*version),
            Err(e) => Outcome::Rejected(e.clone()),
        })
    }

    /// Commits `first`, syncs it, then commits `second`, through one pipeline of partition `id`, whose conflict history
    /// holds what `first` wrote when `second` is decided: how the commit step answered each.
    pub fn commit_in_turn(
        &mut self,
        id: u32,
        first: Prepared,
        second: Prepared,
    ) -> TestResult<(Outcome, Outcome)> {
        let (one, two) = (self.ticket + 1, self.ticket + 2);
        self.ticket += 2;
        let ((), replies) = self.with_pipe(id, |_, pipe, _| {
            pipe.commit(one, first, 0)?;
            block_on(pipe.sync(0))??;
            Ok(pipe.commit(two, second, 0)?)
        })?;
        let outcome = |ticket| -> TestResult<Outcome> {
            let reply = replies
                .iter()
                .find(|r| r.ticket() == ticket)
                .context("the commit is answered")?;
            Ok(match reply.outcome() {
                Ok(version) => Outcome::Committed(*version),
                Err(e) => Outcome::Rejected(e.clone()),
            })
        };
        Ok((outcome(one)?, outcome(two)?))
    }

    /// Restarts partition `id` over its store: a new applier opened over the same store, as a restarted process opens
    /// it, its home's state and dictionary cache gone.
    pub fn restart(&mut self, id: u32) -> TestResult {
        let at = id as usize - 1;
        let old = self.parts.get(at).context("the partition exists")?;
        let store = old.applier().quads().store().clone();
        let placement = old.placement().clone();
        let seed = Seed::new(u64::from(id) + 100);
        let cache = SimListCache::new(1024, seed.stream(Stream::Cache));
        let quads = QuadStore::open(
            Lists::new(store, cache, ListsConfig::default()),
            TestBlockSource::new(NonZeroU64::MIN.saturating_add(999)),
            RandomSource::new(u64::from(id) + 100),
        )?
        .owning(Part::owned(pid(id)?, &placement, self.closed).context("the map's ranges")?);
        let applier = ChangesetApplier::over(
            quads,
            SteppedExecutor::new(seed.stream(Stream::Executor)),
            1,
        )?;
        if let Some(slot) = self.parts.get_mut(at) {
            *slot = Partition::new(pid(id)?, placement, applier);
        }
        if let Some(home) = self.homes.get_mut(at) {
            *home = Home::new();
        }
        Ok(())
    }

    /// Runs `read` on a read-write transaction of partition `id` at its latest state, then aborts it.
    pub fn read<T>(
        &self,
        id: u32,
        read: impl FnOnce(&mut Txn<'_>) -> Result<T, TxnError>,
    ) -> TestResult<Result<T, TxnError>> {
        let mut txn = begin(self.part(id)?)?;
        let result = read(&mut txn);
        if !txn.is_ended() {
            let _buffers: TxnBuffers = txn.abort();
        }
        Ok(result)
    }

    /// The ID partition `id`'s dictionary holds for `<name>`, if any.
    pub fn id_at(&self, id: u32, name: &str) -> TestResult<Option<SystemId>> {
        Ok(self
            .part(id)?
            .applier()
            .quads()
            .id_of(TENANT, ex(name).as_str())?)
    }

    /// Partition `id`'s store's current contents.
    pub fn contents(&self, id: u32) -> TestResult<ultragraphdb_store_sim::Contents> {
        Ok(self.part(id)?.applier().quads().store().contents())
    }
}

/// Partition `id` of `placement`, over a fault-free simulated store of its own, owning its buckets at `first`.
pub fn node(id: PartitionId, placement: &PlacementMap, first: Epoch) -> TestResult<Part> {
    let seed = Seed::new(u64::from(id.get()));
    let store = SimStore::new(seed, FaultConfig::none(), SimClock::new());
    let cache = SimListCache::new(1024, seed.stream(Stream::Cache));
    let quads = QuadStore::open(
        Lists::new(store, cache, ListsConfig::default()),
        TestBlockSource::new(NonZeroU64::MIN.saturating_add(999)),
        RandomSource::new(u64::from(id.get())),
    )?
    .owning(Part::owned(id, placement, first).context("the map's ranges")?);
    let applier = ChangesetApplier::over(
        quads,
        SteppedExecutor::new(seed.stream(Stream::Executor)),
        1,
    )?;
    Ok(Partition::new(id, placement.clone(), applier))
}

/// A read-write transaction of [`TENANT`] at partition `part`'s watermark, bound to it.
pub fn begin(part: &Part) -> TestResult<Txn<'_>> {
    Ok(part.begin(
        TENANT,
        Mode::ReadWrite,
        0,
        0,
        TxnLimits::default(),
        TxnBuffers::new(),
    )?)
}
