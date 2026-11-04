/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the unit tests and the scenarios share: real transactions on a changeset applier over a fault-free simulated
//! store, a driver standing in for the pipeline (it appends nothing, counts the rounds it is asked to append and the
//! syncs it completes, and applies each synced round with `apply_round`), and a seeded generator.

use std::num::NonZeroU64;

use oxrdf::{GraphName, Literal, NamedNode, Term};
use ultragraphdb_cluster_sequencer::{CatalogVersion, Close};
use ultragraphdb_core_ids::{BucketGroup, Epoch, SystemId};
use ultragraphdb_core_mapping::{Change, Changeset, RandomSource};
use ultragraphdb_store_apply::{Applied, ChangesetApplier, EntityRef, Executed};
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_sim::{
    FaultConfig, Seed, SimClock, SimListCache, SimStore, SteppedExecutor, Stream,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_traits_blocks::TestBlockSource;
use ultragraphdb_transactions_commit::{
    CommitRequest, CommitStep, Input, Reply, RoundBounds, RoundRecord, StepOutput,
};
use ultragraphdb_transactions_log::{SimStorage, Wal, WalConfig};
use ultragraphdb_transactions_txn::{Commit, Mode, Prepared, Transaction, TxnBuffers, TxnLimits};

/// The write-ahead log of the tests, over simulated storage.
pub type Log = Wal<SimStorage>;
/// The changeset applier of the tests: over the simulated store, its cache and its stepped executor.
pub type Applier =
    ChangesetApplier<SimStore, SimListCache<CachedList>, SteppedExecutor, TestBlockSource>;
/// A transaction on the tests' applier.
pub type Txn<'a> =
    Transaction<'a, SimStore, SimListCache<CachedList>, SteppedExecutor, TestBlockSource>;

/// The namespace the tests write.
pub const NS: u64 = 0;

/// A fault-free simulated store.
pub fn sim_store() -> SimStore {
    SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new())
}

/// A changeset applier over a fault-free simulated store.
pub fn applier() -> TestResult<Applier> {
    applier_over(sim_store())
}

/// A changeset applier over `store`, as a restarted node opens it.
pub fn applier_over(store: SimStore) -> TestResult<Applier> {
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    let executor = SteppedExecutor::new(Seed::new(1).stream(Stream::Executor));
    // Blocks of 1,000 sequences stand in for the metadata group's leases; a restarted node leases afresh.
    ChangesetApplier::new(
        Lists::new(store, cache, ListsConfig::default()),
        TestBlockSource::new(NonZeroU64::MIN.saturating_add(999)),
        RandomSource::new(1),
        executor,
        1,
    )
    .context("the applier opens")
}

/// A fresh write-ahead log over simulated storage, and the storage, to inject its faults.
pub fn wal() -> TestResult<(SimStorage, Log)> {
    futures::executor::block_on(open_wal())
}

/// The write-ahead log over `storage` opened again, as a restarted node opens it, and what its open found.
pub fn reopen_wal(
    storage: &SimStorage,
) -> TestResult<(Log, ultragraphdb_transactions_log::Recovery)> {
    futures::executor::block_on(reopen(storage))
}

/// The same, awaited: for a caller already on an executor.
pub async fn reopen(
    storage: &SimStorage,
) -> TestResult<(Log, ultragraphdb_transactions_log::Recovery)> {
    Wal::open(storage.clone(), "/wal", WalConfig::new(1, true))
        .await
        .context("the log opens again")
}

/// The same, awaited: for a caller already on an executor.
pub async fn open_wal() -> TestResult<(SimStorage, Log)> {
    let storage = SimStorage::new(1);
    let (wal, _) = Wal::open(storage.clone(), "/wal", WalConfig::new(1, true))
        .await
        .context("the log opens")?;
    Ok((storage, wal))
}

/// The IRI `http://ex.org/<name>`.
pub fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

/// The ID the store holds for `<name>`.
pub fn id_of(applier: &Applier, name: &str) -> TestResult<SystemId> {
    applier
        .quads()
        .id_of(NS, ex(name).as_str())
        .context("the dictionary reads")?
        .context("the IRI is known")
}

/// The ID `prepared`'s execution gives `<name>`, an IRI it created or took from a concurrent creator: one whose
/// dictionary entries its round writes.
pub fn id_in(prepared: &Prepared, name: &str) -> TestResult<SystemId> {
    let iri = ex(name);
    prepared
        .executed()
        .terms()
        .written()
        .find(|(term, _)| *term == iri.as_str())
        .map(|(_, id)| id)
        .context("the execution writes the IRI's entries")
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

/// The delete of `<subject> <predicate> "value"`.
pub fn delete_literal(subject: &str, predicate: &str, value: &str) -> Change {
    Change::Delete {
        graph: GraphName::DefaultGraph,
        subject: ex(subject).into(),
        predicate: ex(predicate),
        object: Literal::new_simple_literal(value).into(),
    }
}

/// A changeset of `NS` holding `changes`.
pub fn changeset(changes: Vec<Change>) -> Changeset {
    changes
        .into_iter()
        .fold(Changeset::new(NS), |cs, change| cs.with(change))
}

/// Commits `changes` on their own, outside the commit step, as set-up: its commit version.
pub fn setup(applier: &Applier, changes: Vec<Change>) -> TestResult<u64> {
    Ok(applier
        .apply(&changeset(changes))
        .context("the set-up applies")?
        .commit_ts())
}

/// Begins a read-write transaction of `NS` at the watermark.
pub fn begin(applier: &Applier) -> TestResult<Txn<'_>> {
    begin_with(applier, TxnBuffers::new())
}

/// Begins a read-write transaction of `NS` at the watermark on `buffers`.
pub fn begin_with(applier: &Applier, buffers: TxnBuffers) -> TestResult<Txn<'_>> {
    Transaction::begin(
        applier,
        NS,
        Mode::ReadWrite,
        0,
        0,
        TxnLimits::default(),
        buffers,
    )
    .context("it begins")
}

/// The objects of `<subject> <predicate>` that `txn` reads.
pub fn read(txn: &mut Txn<'_>, subject: &str, predicate: &str) -> TestResult<Vec<Term>> {
    let mut out = Vec::new();
    let s = ex(subject);
    let p = ex(predicate);
    txn.objects_into(
        GraphName::DefaultGraph.as_ref(),
        s.as_ref().into(),
        p.as_ref(),
        0,
        &mut out,
    )
    .context("the read")?;
    Ok(out)
}

/// Writes `changes` in `txn`.
pub fn write(txn: &mut Txn<'_>, changes: Vec<Change>) -> TestResult {
    txn.write(&changeset(changes), 0).context("the write")
}

/// Ends a read-write `txn` as its proposal.
pub fn prepare(txn: Txn<'_>) -> TestResult<Prepared> {
    prepare_in(txn, 0, 0)
}

/// Ends a read-write `txn` as its proposal, request `request` of session `session`.
pub fn prepare_in(txn: Txn<'_>, session: u128, request: u64) -> TestResult<Prepared> {
    let prepared = match txn
        .commit(session, request, &[], 0)
        .context("it prepares")?
    {
        Commit::ReadWrite(prepared) => Some(prepared),
        Commit::ReadOnly(_) => None,
    };
    prepared.context("a read-write transaction prepares")
}

/// A registration of `<name>` from bucket group `group` the partition of `applier` accepted as the IRI's home, the
/// member's generation `generation`: the entity's ID, what its round applies ([`ultragraphdb_store_apply::Registering::finish`])
/// and its record of kind 2.
pub fn registered(
    applier: &Applier,
    name: &str,
    group: u32,
    generation: u64,
) -> TestResult<(SystemId, Executed, Vec<u8>)> {
    use ultragraphdb_protocol_wire::{RecordRegistration, WriteSetRecord};
    let iri = ex(name);
    let registering = applier
        .register(NS, EntityRef::Iri(iri.as_str()))
        .context("the home accepts the registration")?;
    let entity = registering.entity();
    let group = BucketGroup::new(group).context("the group")?;
    let generation = NonZeroU64::new(generation).context("a generation")?;
    let executed = registering.finish(group, generation);
    let record =
        WriteSetRecord::registration(NS, RecordRegistration::new(entity, group, generation))
            .encode();
    Ok((entity, executed, record))
}

/// What an execution writing nothing leaves, at the watermark: a registration's round that changes no member.
pub fn nothing(applier: &Applier) -> TestResult<Executed> {
    let watermark = applier.quads().lists().applied_ts();
    let execution = applier.begin(NS, watermark).context("it begins")?;
    execution.finish().context("it finishes")
}

/// The generation and epoch of the member of `group` in the referrer set of `entity` the store holds as of `version`;
/// `None` when the set names no such member.
pub fn member_at(
    applier: &Applier,
    entity: SystemId,
    group: u32,
    version: u64,
) -> TestResult<Option<(u64, u64)>> {
    let group = BucketGroup::new(group).context("the group")?;
    let set = applier
        .quads()
        .referrers(NS, entity, version)
        .context("the referrer set reads")?;
    Ok(set
        .member(group)
        .map(|m| (m.generation().get(), m.epoch().get())))
}

/// What a fresh transaction reads of `<subject> <predicate>`: what the store holds at the watermark.
pub fn held(applier: &Applier, subject: &str, predicate: &str) -> TestResult<Vec<Term>> {
    let mut txn = begin(applier)?;
    read(&mut txn, subject, predicate)
}

/// The commit step over the applier, and what a pipeline would do with its outputs.
pub struct Driver<'a> {
    /// The applier each synced round is applied to.
    pub applier: &'a Applier,
    /// The commit step under test.
    pub step: CommitStep<Prepared>,
    /// The step's output, read and drained after every input.
    pub out: StepOutput<Prepared>,
    /// The rounds the step asked to append, in order.
    pub appended: Vec<RoundRecord>,
    /// The syncs completed.
    pub syncs: usize,
    /// Every reply, in order.
    pub replies: Vec<Reply>,
    /// The buffers of every proposal the step released, under its ticket, in order: each released on the applier, as
    /// the pipeline releases it.
    pub released: Vec<(u64, TxnBuffers)>,
    /// The clock the step is handed with every input, microseconds.
    pub now_us: u64,
    /// The closes the step asked the sequencer for, in order.
    pub close_requests: Vec<Epoch>,
}

impl<'a> Driver<'a> {
    /// A driver whose commit step follows the applier's watermark, with `bounds`.
    pub fn new(applier: &'a Applier, bounds: RoundBounds) -> Self {
        let watermark = applier.quads().lists().applied_ts();
        Self {
            applier,
            step: CommitStep::new(bounds, watermark),
            out: StepOutput::new(),
            appended: Vec::new(),
            syncs: 0,
            replies: Vec::new(),
            released: Vec::new(),
            now_us: 0,
            close_requests: Vec::new(),
        }
    }

    fn run(&mut self, input: Input<Prepared>) -> TestResult {
        self.step.step(input, self.now_us, &mut self.out);
        if let Some((record, _)) = self.out.append() {
            self.appended.push(record);
        }
        let round: Vec<_> = self
            .out
            .apply_mut()
            .drain(..)
            .map(|(version, prepared)| (version, prepared.into_parts().0))
            .collect();
        if !round.is_empty() {
            let _applied: Vec<Applied> = self
                .applier
                .apply_round(round)
                .context("the round applies")?;
        }
        self.replies.extend(self.out.replies().iter().cloned());
        let released = self
            .out
            .released_mut()
            .drain(..)
            .map(|(ticket, prepared)| (ticket, prepared.release(self.applier)));
        self.released.extend(released);
        self.close_requests.extend(self.out.close_request());
        Ok(())
    }

    /// Delivers the close of `epoch`, announcing no catalog version and evicting no session.
    pub fn close(&mut self, epoch: u64) -> TestResult {
        self.close_with(epoch, Vec::new(), Vec::new())
    }

    /// Delivers the close of `epoch` announcing `catalog` and carrying `evictions`.
    pub fn close_with(
        &mut self,
        epoch: u64,
        catalog: Vec<CatalogVersion>,
        evictions: Vec<u128>,
    ) -> TestResult {
        self.run(Input::Close(close(epoch, catalog, evictions)?))
    }

    /// Steps the clock to `now_us` with nothing else to step on.
    pub fn tick(&mut self, now_us: u64) -> TestResult {
        self.now_us = now_us;
        self.run(Input::Tick)
    }

    /// Hands `prepared` to the commit step under `ticket`.
    pub fn submit(&mut self, ticket: u64, prepared: Prepared) -> TestResult {
        self.run(Input::Commit(CommitRequest::new(ticket, prepared)))
    }

    /// Completes the sync of the round in flight.
    pub fn sync(&mut self) -> TestResult {
        self.syncs += 1;
        self.run(Input::Synced)
    }

    /// The reply to `ticket`, if one was sent.
    pub fn reply(&self, ticket: u64) -> Option<&Reply> {
        self.replies.iter().find(|r| r.ticket() == ticket)
    }
}

/// SplitMix64, the property tests' generator: a fixed seed replays a run.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    /// The generator seeded with `seed`.
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// The next number of the sequence.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number below `n`, which is not zero.
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    /// A key of 1 to `max_len` bytes over a three-letter alphabet, so that keys repeat and share prefixes.
    pub fn key(&mut self, max_len: u64) -> Vec<u8> {
        let len = 1 + self.below(max_len);
        (0..len).map(|_| b'a' + self.below(3) as u8).collect()
    }
}

/// The error a result holds, for a test that expects a refusal.
pub trait Refused<E> {
    /// The error, or a failure naming `what` was expected and the value found instead.
    fn refused(self, what: &str) -> TestResult<E>;
}

impl<T: std::fmt::Debug, E> Refused<E> for Result<T, E> {
    #[track_caller]
    fn refused(self, what: &str) -> TestResult<E> {
        let at = std::panic::Location::caller();
        match self {
            Ok(value) => Err(TestError::new(format!(
                "{what}: not refused, found {value:?} (at {at})"
            ))),
            Err(error) => Ok(error),
        }
    }
}

/// The sequencer's close of `epoch`, at time 0, announcing `catalog` and carrying `evictions`.
pub fn close(epoch: u64, catalog: Vec<CatalogVersion>, evictions: Vec<u128>) -> TestResult<Close> {
    Close::new(
        Epoch::new(epoch).context("the epoch")?,
        0,
        catalog,
        evictions,
    )
    .context("the close")
}

/// The commit version of `epoch` and `sequence`, written from the layout: `epoch << 22 | sequence`.
pub fn version(epoch: u64, sequence: u64) -> u64 {
    (epoch << 22) | sequence
}

/// `replies` without their session tokens, for a test about what else they answer: tokens are tested on their own.
pub fn plain(replies: &[Reply]) -> Vec<Reply> {
    replies
        .iter()
        .map(|r| {
            Reply::new(r.ticket(), r.outcome().clone())
                .with_results(r.results().to_vec(), r.terms().clone())
        })
        .collect()
}

/// Version `version` of `namespace`'s catalog declaring `predicate` with a `term` index.
pub fn term_version(
    namespace: u64,
    version: u64,
    predicate: &str,
) -> ultragraphdb_core_catalog::CatalogContents {
    use ultragraphdb_core_mapping::{Declaration, PredicateDeclaration};
    let declared =
        PredicateDeclaration::new(ex(predicate)).with_tokenizers(vec!["term".to_string()]);
    ultragraphdb_core_catalog::CatalogContents::new(
        namespace,
        version,
        vec![Declaration::Predicate(declared)],
    )
}

/// Catalog version `contents` taking effect on `applier` as a proposal of the commit step, executed at the watermark;
/// none of its declarations refused.
pub fn catalog_proposal(
    applier: &Applier,
    contents: &ultragraphdb_core_catalog::CatalogContents,
) -> TestResult<Prepared> {
    let (prepared, refusals) = Prepared::catalog(applier, contents, TxnBuffers::new())?;
    if !refusals.is_empty() {
        return Err(ultragraphdb_tool_bdd::TestError::new(format!(
            "refused: {refusals:?}"
        )));
    }
    Ok(prepared)
}
