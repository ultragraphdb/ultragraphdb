/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the unit tests and the scenarios share: a changeset applier over a fault-free simulated store, statements
//! named by short IRIs, commits that pass the watermark, and a seeded generator for the property tests.

use std::num::NonZeroU64;

use oxrdf::{GraphName, Literal, NamedNode, Term};
use ultragraphdb_core_ids::{Sequence, SystemId, TypePrefix};
use ultragraphdb_core_keys::{Graph, QuadKey};
use ultragraphdb_core_mapping::{Change, Changeset, RandomSource};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_apply::ChangesetApplier;
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_quads::QuadWrite;
use ultragraphdb_store_sim::{
    FaultConfig, Seed, SimClock, SimListCache, SimStore, SteppedExecutor, Stream,
};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_blocks::TestBlockSource;
use ultragraphdb_transactions_txn::{
    Commit, Mode, Prepared, Transaction, TxnBuffers, TxnError, TxnLimits,
};

/// The changeset applier of the tests: over the simulated store, its cache and its stepped executor.
pub type Applier =
    ChangesetApplier<SimStore, SimListCache<CachedList>, SteppedExecutor, TestBlockSource>;
/// A transaction on the tests' applier.
pub type Txn<'a> =
    Transaction<'a, SimStore, SimListCache<CachedList>, SteppedExecutor, TestBlockSource>;

/// The tenant the tests write.
pub const TENANT: u64 = 0;

/// One second, in the microseconds a transaction's clock counts.
pub const SECOND: u64 = 1_000_000;

/// A changeset applier over a fault-free simulated store.
pub fn applier() -> TestResult<Applier> {
    let store = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    let executor = SteppedExecutor::new(Seed::new(1).stream(Stream::Executor));
    // Blocks of 1,000 sequences stand in for the metadata group's leases.
    ChangesetApplier::new(
        Lists::new(store, cache, ListsConfig::default()),
        TestBlockSource::new(NonZeroU64::MIN.saturating_add(999)),
        RandomSource::new(1),
        executor,
        1,
    )
    .context("the applier opens")
}

/// The ID `prepared`'s execution gives `<name>`, an IRI it created or took from a concurrent creator: one whose
/// dictionary entries its commit writes.
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

/// The IRI `http://ex.org/<name>`.
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

/// A changeset of `TENANT` holding `changes`.
pub fn changeset(changes: Vec<Change>) -> Changeset {
    changes
        .into_iter()
        .fold(Changeset::new(TENANT), |cs, change| cs.with(change))
}

/// Commits `changes` on their own, as an acknowledged commit: the watermark passes it. Its commit version.
pub fn commit(applier: &Applier, changes: Vec<Change>) -> TestResult<u64> {
    Ok(applier
        .apply(&changeset(changes))
        .context("the commit applies")?
        .commit_ts())
}

/// Commits `changes` as [`commit`] does, where the test needs no commit version.
pub fn store(applier: &Applier, changes: Vec<Change>) -> TestResult {
    commit(applier, changes).map(|_version| ())
}

/// `<a> <knows> <b>` held under `count` statement IDs of sequences above every lease of the tests, in both its postings: the IDs.
pub fn held_under(applier: &Applier, count: usize) -> TestResult<Vec<SystemId>> {
    store(applier, vec![insert("a", "knows", "b")])?;
    let quads = applier.quads();
    let id = |name: &str| -> TestResult<SystemId> {
        quads.id_of(TENANT, ex(name).as_str())?.context("an ID")
    };
    let (subject, knows, object) = (id("a")?, id("knows")?, EncodedTerm::id(id("b")?));
    let ids = (1..=u64::try_from(count)?)
        .map(|n| {
            Ok(SystemId::new(
                TypePrefix::STATEMENT,
                subject.bucket(),
                Sequence::new((1 << 40) + n)?,
            ))
        })
        .collect::<TestResult<Vec<_>>>()?;
    let mut batch = quads.lists().batch();
    let mut write = QuadWrite::new(quads, TENANT, batch.request(quads.lists().applied_ts()));
    write.set_statement_ids(Graph::Default, subject, knows, &object, &ids);
    let (request, terms) = write.finish()?;
    batch.push(request);
    let _committed: Vec<u64> = quads.commit(batch, &[terms])?;
    Ok(ids)
}

/// What `applier` holds of `<a> <knows> ?` and `? <knows> <b>`, each posting with its statement IDs.
pub fn both_postings(applier: &Applier) -> TestResult<Vec<(EncodedTerm, Vec<SystemId>)>> {
    let quads = applier.quads();
    let id = |name: &str| -> TestResult<SystemId> {
        quads.id_of(TENANT, ex(name).as_str())?.context("an ID")
    };
    let (subject, knows, object) = (id("a")?, id("knows")?, EncodedTerm::id(id("b")?));
    let forward = QuadKey::Forward {
        tenant: TENANT,
        graph: Graph::Default,
        predicate: knows,
        subject,
    };
    let reverse = QuadKey::Reverse {
        tenant: TENANT,
        graph: Graph::Default,
        predicate: knows,
        object,
    };
    let version = quads.lists().applied_ts();
    let mut held = quads.objects(&forward.to_bytes(), version)?;
    held.extend(quads.objects(&reverse.to_bytes(), version)?);
    Ok(held)
}

/// A parallel edge of `<a> <knows> <b>`.
pub fn parallel_edge() -> Change {
    Change::InsertEdge {
        graph: GraphName::DefaultGraph,
        subject: ex("a").into(),
        predicate: ex("knows"),
        object: ex("b").into(),
        properties: Vec::new(),
    }
}

/// Begins a transaction of `TENANT` at the watermark at time 0 with the default limits.
pub fn begin(applier: &Applier, mode: Mode) -> TestResult<Txn<'_>> {
    begin_with(applier, mode, TxnLimits::default())
}

/// Begins a transaction of `TENANT` at the watermark at time 0 with `limits`.
pub fn begin_with(applier: &Applier, mode: Mode, limits: TxnLimits) -> TestResult<Txn<'_>> {
    Transaction::begin(applier, TENANT, mode, 0, 0, limits, TxnBuffers::new()).context("it begins")
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

/// Reads `<subject> <predicate>` in `txn` at `now_us` as [`objects`] does, recording the read, where the test needs
/// none of the objects.
pub fn read_objects(
    txn: &mut Txn<'_>,
    subject: &str,
    predicate: &str,
    now_us: u64,
) -> Result<(), TxnError> {
    objects(txn, subject, predicate, now_us).map(|_objects| ())
}

/// What a fresh read-only transaction reads of `<subject> <predicate>`: what the store holds now.
pub fn held(applier: &Applier, subject: &str, predicate: &str) -> TestResult<Vec<Term>> {
    let mut txn = begin(applier, Mode::ReadOnly)?;
    objects(&mut txn, subject, predicate, 0).context("the read")
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

    /// A key of 0 to `max_len` bytes over a small alphabet, so that keys repeat and share prefixes.
    pub fn key(&mut self, max_len: u64) -> Vec<u8> {
        let len = self.below(max_len + 1);
        // Below 3, so the low byte is the whole draw.
        (0..len)
            .map(|_| b'a' + self.below(3).to_le_bytes()[0])
            .collect()
    }
}

/// The proposal a read-write transaction's commit hands the commit step.
pub fn prepared(commit: Result<Commit, TxnError>) -> TestResult<Prepared> {
    let proposal = match commit.context("the commit")? {
        Commit::ReadWrite(prepared) => Some(prepared),
        Commit::ReadOnly(_) => None,
    };
    proposal.context("a read-write transaction ends on its worker")
}
