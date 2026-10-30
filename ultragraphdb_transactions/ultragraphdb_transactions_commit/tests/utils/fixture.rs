/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the unit tests and the scenarios share: real transactions on a changeset applier over a fault-free simulated
//! store, a driver standing in for the pipeline (it appends nothing, counts the rounds it is asked to append and the
//! syncs it completes, and applies each synced round with `apply_round`), and a seeded generator.

use oxrdf::{GraphName, Literal, NamedNode, Term};
use ultragraphdb_core_mapping::{Change, Changeset};
use ultragraphdb_store_apply::ChangesetApplier;
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_sim::{
    FaultConfig, Seed, SimClock, SimListCache, SimStore, SteppedExecutor, Stream,
};
use ultragraphdb_transactions_commit::{
    CommitRequest, CommitStep, Input, Reply, RoundBounds, RoundRecord, StepOutput,
};
use ultragraphdb_transactions_log::{SimStorage, Wal, WalConfig};
use ultragraphdb_transactions_txn::{Commit, Mode, Prepared, Transaction, TxnBuffers, TxnLimits};

pub type Log = Wal<SimStorage>;
pub type Applier = ChangesetApplier<SimStore, SimListCache<CachedList>, SteppedExecutor>;
pub type Txn<'a> = Transaction<'a, SimStore, SimListCache<CachedList>>;

/// The namespace the tests write.
pub const NS: u64 = 0;

/// A fault-free simulated store.
pub fn sim_store() -> SimStore {
    SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new())
}

/// A changeset applier over a fault-free simulated store.
pub fn applier() -> Applier {
    applier_over(sim_store())
}

/// A changeset applier over `store`, as a restarted node opens it.
pub fn applier_over(store: SimStore) -> Applier {
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    let executor = SteppedExecutor::new(Seed::new(1).stream(Stream::Executor));
    ChangesetApplier::new(
        Lists::new(store, cache, ListsConfig::default()),
        executor,
        1,
    )
    .expect("the applier opens")
}

/// A fresh write-ahead log over simulated storage, and the storage, to inject its faults.
pub fn wal() -> (SimStorage, Log) {
    futures::executor::block_on(open_wal())
}

/// The write-ahead log over `storage` opened again, as a restarted node opens it, and what its open found.
pub fn reopen_wal(storage: &SimStorage) -> (Log, ultragraphdb_transactions_log::Recovery) {
    futures::executor::block_on(reopen(storage))
}

/// The same, awaited: for a caller already on an executor.
pub async fn reopen(storage: &SimStorage) -> (Log, ultragraphdb_transactions_log::Recovery) {
    Wal::open(storage.clone(), "/wal", WalConfig::new(1, true))
        .await
        .expect("the log opens again")
}

/// The same, awaited: for a caller already on an executor.
pub async fn open_wal() -> (SimStorage, Log) {
    let storage = SimStorage::new(1);
    let (wal, _) = Wal::open(storage.clone(), "/wal", WalConfig::new(1, true))
        .await
        .expect("the log opens");
    (storage, wal)
}

pub fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

/// The UID the store holds for `<name>`.
pub fn uid_of(applier: &Applier, name: &str) -> u64 {
    applier
        .quads()
        .uid_of(NS, ex(name).as_str())
        .expect("the dictionary reads")
        .expect("the IRI is known")
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
pub fn setup(applier: &Applier, changes: Vec<Change>) -> u64 {
    applier
        .apply(&changeset(changes))
        .expect("the set-up applies")
        .commit_ts()
}

/// Begins a read-write transaction of `NS` at the watermark.
pub fn begin(applier: &Applier) -> Txn<'_> {
    begin_with(applier, TxnBuffers::new())
}

/// Begins a read-write transaction of `NS` at the watermark on `buffers`.
pub fn begin_with(applier: &Applier, buffers: TxnBuffers) -> Txn<'_> {
    Transaction::begin(
        applier,
        NS,
        Mode::ReadWrite,
        0,
        0,
        TxnLimits::default(),
        buffers,
    )
    .expect("it begins")
}

/// The objects of `<subject> <predicate>` that `txn` reads.
pub fn read(txn: &mut Txn<'_>, subject: &str, predicate: &str) -> Vec<Term> {
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
    .expect("the read");
    out
}

/// Writes `changes` in `txn`.
pub fn write(txn: &mut Txn<'_>, changes: Vec<Change>) {
    txn.write(&changeset(changes), 0).expect("the write");
}

/// Ends a read-write `txn` as its proposal.
pub fn prepare(txn: Txn<'_>) -> Prepared {
    prepare_in(txn, 0, 0)
}

/// Ends a read-write `txn` as its proposal, request `request` of session `session`.
pub fn prepare_in(txn: Txn<'_>, session: u64, request: u64) -> Prepared {
    match txn.commit(session, request, &[], 0).expect("it prepares") {
        Commit::ReadWrite(prepared) => prepared,
        Commit::ReadOnly(_) => panic!("a read-write transaction prepares"),
    }
}

/// What a fresh transaction reads of `<subject> <predicate>`: what the store holds at the watermark.
pub fn held(applier: &Applier, subject: &str, predicate: &str) -> Vec<Term> {
    let mut txn = begin(applier);
    read(&mut txn, subject, predicate)
}

/// The commit step over the applier, and what a pipeline would do with its outputs.
pub struct Driver<'a> {
    pub applier: &'a Applier,
    pub step: CommitStep<Prepared>,
    pub out: StepOutput<Prepared>,
    /// The rounds the step asked to append, in order.
    pub appended: Vec<RoundRecord>,
    /// The syncs completed.
    pub syncs: usize,
    /// Every reply, in order.
    pub replies: Vec<Reply>,
    /// The buffers of every proposal the step released, under its ticket, in order.
    pub released: Vec<(u64, TxnBuffers)>,
    pub now_us: u64,
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
        }
    }

    fn run(&mut self, input: Input<Prepared>) {
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
            self.applier.apply_round(round).expect("the round applies");
        }
        self.replies.extend(self.out.replies().iter().cloned());
        let released = self
            .out
            .released_mut()
            .drain(..)
            .map(|(ticket, prepared)| (ticket, prepared.into_parts().1));
        self.released.extend(released);
    }

    /// Hands `prepared` to the commit step under `ticket`.
    pub fn submit(&mut self, ticket: u64, prepared: Prepared) {
        self.run(Input::Commit(CommitRequest::new(ticket, prepared)));
    }

    /// Completes the sync of the round in flight.
    pub fn sync(&mut self) {
        self.syncs += 1;
        self.run(Input::Synced);
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
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number below `n`, which is not zero.
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// A key of 1 to `max_len` bytes over a three-letter alphabet, so that keys repeat and share prefixes.
    pub fn key(&mut self, max_len: u64) -> Vec<u8> {
        let len = 1 + self.below(max_len);
        (0..len).map(|_| b'a' + self.below(3) as u8).collect()
    }
}
