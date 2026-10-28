/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the unit tests and the scenarios share: a changeset applier over a fault-free simulated store, statements
//! named by short IRIs, commits that pass the watermark, and a seeded generator for the property tests.

use oxrdf::{GraphName, Literal, NamedNode, Term};
use ultragraphdb_core_mapping::{Change, Changeset};
use ultragraphdb_store_apply::ChangesetApplier;
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_sim::{
    FaultConfig, Seed, SimClock, SimListCache, SimStore, SteppedExecutor, Stream,
};
use ultragraphdb_transactions_txn::{Mode, Transaction, TxnBuffers, TxnError, TxnLimits};

pub type Applier = ChangesetApplier<SimStore, SimListCache<CachedList>, SteppedExecutor>;
pub type Txn<'a> = Transaction<'a, SimStore, SimListCache<CachedList>>;

/// The namespace the tests write.
pub const NS: u64 = 0;

/// One second, in the microseconds a transaction's clock counts.
pub const SECOND: u64 = 1_000_000;

/// A changeset applier over a fault-free simulated store.
pub fn applier() -> Applier {
    let store = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    let executor = SteppedExecutor::new(Seed::new(1).stream(Stream::Executor));
    ChangesetApplier::new(
        Lists::new(store, cache, ListsConfig::default()),
        executor,
        1,
    )
    .expect("the applier opens")
}

pub fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
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

/// A changeset of `NS` holding `changes`.
pub fn changeset(changes: Vec<Change>) -> Changeset {
    changes
        .into_iter()
        .fold(Changeset::new(NS), |cs, change| cs.with(change))
}

/// Commits `changes` on their own, as an acknowledged commit: the watermark passes it. Its commit version.
pub fn commit(applier: &Applier, changes: Vec<Change>) -> u64 {
    applier
        .apply(&changeset(changes))
        .expect("the commit applies")
        .commit_ts()
}

/// Begins a transaction of `NS` at the watermark at time 0 with the default limits.
pub fn begin(applier: &Applier, mode: Mode) -> Txn<'_> {
    begin_with(applier, mode, TxnLimits::default())
}

/// Begins a transaction of `NS` at the watermark at time 0 with `limits`.
pub fn begin_with(applier: &Applier, mode: Mode, limits: TxnLimits) -> Txn<'_> {
    Transaction::begin(applier, NS, mode, 0, 0, limits, TxnBuffers::new()).expect("it begins")
}

/// The objects of `<subject> <predicate>` in the default graph that `txn` reads at `now_us`.
pub fn objects(
    txn: &mut Txn<'_>,
    subject: &str,
    predicate: &str,
    now_us: u64,
) -> Result<Vec<Term>, TxnError> {
    let mut out = Vec::new();
    let s = ex(subject);
    let p = ex(predicate);
    txn.objects_into(
        GraphName::DefaultGraph.as_ref(),
        s.as_ref().into(),
        p.as_ref(),
        now_us,
        &mut out,
    )?;
    Ok(out)
}

/// What a fresh read-only transaction reads of `<subject> <predicate>`: what the store holds now.
pub fn held(applier: &Applier, subject: &str, predicate: &str) -> Vec<Term> {
    let mut txn = begin(applier, Mode::ReadOnly);
    objects(&mut txn, subject, predicate, 0).expect("the read")
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

    /// A key of 0 to `max_len` bytes over a small alphabet, so that keys repeat and share prefixes.
    pub fn key(&mut self, max_len: u64) -> Vec<u8> {
        let len = self.below(max_len + 1);
        (0..len).map(|_| b'a' + self.below(3) as u8).collect()
    }
}
