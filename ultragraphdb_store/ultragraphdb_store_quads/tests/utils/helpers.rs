/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the unit tests and the scenarios share: quads over a fault-free simulated store, leasing from a test block
//! source, and a write of quads.

use std::num::NonZeroU64;

use oxrdf::{GraphName, NamedNode, NamedOrBlankNode, Quad, Term};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{Graph, QuadKey};
use ultragraphdb_core_mapping::RandomSource;
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_quads::{QuadStore, QuadWrite, WriteTerms};
use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimListCache, SimStore, Stream};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_blocks::TestBlockSource;

pub type TestQuads = QuadStore<SimStore, SimListCache<CachedList>, TestBlockSource>;
pub type TestWrite<'a> = QuadWrite<'a, SimStore, SimListCache<CachedList>, TestBlockSource>;

pub fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

pub fn node(name: &str) -> NamedOrBlankNode {
    ex(name).into()
}

/// A block source handing out blocks of 1,000 sequences.
pub fn blocks() -> TestBlockSource {
    TestBlockSource::new(NonZeroU64::MIN.saturating_add(999))
}

/// A fault-free simulated store.
pub fn sim() -> SimStore {
    SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new())
}

/// The posting lists over `store`, with a fresh cache.
pub fn lists_over(store: SimStore) -> Lists<SimStore, SimListCache<CachedList>> {
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    Lists::new(store, cache, ListsConfig::default())
}

/// Quads over `store`, leasing from `source` and drawing names from a source seeded with `seed`.
pub fn quads_with(store: SimStore, source: TestBlockSource, seed: u64) -> TestResult<TestQuads> {
    QuadStore::open(lists_over(store), source, RandomSource::new(seed)).context("opens")
}

/// Quads over `store`, with blocks of their own and the random seed 7.
pub fn quads_over(store: SimStore) -> TestResult<TestQuads> {
    quads_with(store, blocks(), 7)
}

/// Quads over a fault-free simulated store.
pub fn quads() -> TestResult<TestQuads> {
    quads_over(sim())
}

/// The statistics kind of `object`.
pub fn kind(object: &Term) -> String {
    match object {
        Term::NamedNode(_) => "iri".into(),
        Term::BlankNode(_) => "blank".into(),
        Term::Literal(l) => l.datatype().as_str().to_string(),
        Term::Triple(_) => "triple".into(),
    }
}

/// `quads` inserted by one write of tenant `tenant` at a fresh read timestamp, the write's terms and request kept
/// apart so that a test can commit them as it chooses.
pub fn insert_write<'a>(
    store: &'a TestQuads,
    tenant: u64,
    quads: &[Quad],
) -> TestResult<TestWrite<'a>> {
    let read_ts = store.lists().applied_ts();
    let mut write = QuadWrite::new(store, tenant, store.lists().request(read_ts));
    for q in quads {
        let (terms, quad_store) = write.terms();
        let graph = terms.graph(quad_store, q.graph_name.as_ref())?;
        let subject = terms.node(quad_store, q.subject.as_ref())?;
        let predicate = terms.predicate(quad_store, q.predicate.as_ref())?;
        let object = terms.term(quad_store, q.object.as_ref())?;
        write.insert(graph, subject, predicate, &object, &[], &kind(&q.object));
    }
    Ok(write)
}

/// Commits `writes` in one batch: each write's commit version.
pub fn commit(store: &TestQuads, writes: Vec<TestWrite<'_>>) -> TestResult<Vec<u64>> {
    let mut batch = store.lists().batch();
    let mut terms: Vec<WriteTerms> = Vec::new();
    for write in writes {
        let (request, t) = write.finish();
        batch.push(request);
        terms.push(t);
    }
    store.commit(batch, &terms).context("the batch is written")
}

/// `quads` written and committed in tenant `tenant`: the commit version.
pub fn write_at(store: &TestQuads, tenant: u64, quads: &[Quad]) -> TestResult<u64> {
    let w = insert_write(store, tenant, quads)?;
    commit(store, vec![w])?.first().copied().context("commits")
}

/// `quads` written and committed in tenant `tenant`, for a test that does not read the commit version.
pub fn write(store: &TestQuads, tenant: u64, quads: &[Quad]) -> TestResult {
    let _committed_at: u64 = write_at(store, tenant, quads)?;
    Ok(())
}

/// The ID the dictionary holds for `iri` in tenant `tenant`.
pub fn id(store: &TestQuads, tenant: u64, iri: &NamedNode) -> TestResult<SystemId> {
    store.id_of(tenant, iri.as_str())?.context("an ID")
}

/// The named graph `iri` in tenant `tenant`.
pub fn named(store: &TestQuads, tenant: u64, iri: &NamedNode) -> TestResult<Graph> {
    Ok(Graph::Named(id(store, tenant, iri)?))
}

/// The forward key of `subject`'s `predicate` in `graph`, both named by IRI.
pub fn forward(
    store: &TestQuads,
    tenant: u64,
    graph: Graph,
    subject: &NamedNode,
    predicate: &NamedNode,
) -> TestResult<Vec<u8>> {
    Ok(QuadKey::Forward {
        tenant,
        graph,
        predicate: id(store, tenant, predicate)?,
        subject: id(store, tenant, subject)?,
    }
    .to_bytes())
}

/// The quad `s p o` in the default graph.
pub fn quad(s: &str, p: &str, o: impl Into<Term>) -> Quad {
    Quad::new(node(s), ex(p), o, GraphName::DefaultGraph)
}
