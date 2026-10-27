/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the unit tests and the scenarios share: quads over a fault-free simulated store, and a write of quads.

use oxrdf::{GraphName, NamedNode, NamedOrBlankNode, Quad, Term};
use ultragraphdb_core_keys::QuadKey;
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_quads::{QuadError, QuadStore, QuadWrite, WriteTerms};
use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimListCache, SimStore, Stream};

pub type TestQuads = QuadStore<SimStore, SimListCache<CachedList>>;

pub fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

pub fn node(name: &str) -> NamedOrBlankNode {
    ex(name).into()
}

/// Quads over `store`, with a fresh cache.
pub fn quads_over(store: SimStore) -> TestQuads {
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    QuadStore::open(Lists::new(store, cache, ListsConfig::default())).expect("opens")
}

/// Quads over a fault-free simulated store.
pub fn quads() -> TestQuads {
    quads_over(SimStore::new(
        Seed::new(1),
        FaultConfig::none(),
        SimClock::new(),
    ))
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

/// `quads` inserted by one write of namespace `ns` at a fresh read timestamp, the write's terms and request kept
/// apart so that a test can commit them as it chooses.
pub fn insert_write<'a>(
    store: &'a TestQuads,
    ns: u64,
    quads: &[Quad],
) -> QuadWrite<'a, SimStore, SimListCache<CachedList>> {
    let read_ts = store.lists().applied_ts();
    let mut write = QuadWrite::new(store, ns, store.lists().request(read_ts));
    for q in quads {
        let (terms, quads) = write.terms();
        let (s, l) = (quads.store(), quads.leases());
        let graph = terms.graph(s, l, q.graph_name.as_ref()).unwrap();
        let subject = terms.node(s, l, q.subject.as_ref()).unwrap();
        let predicate = terms.predicate(s, l, q.predicate.as_ref()).unwrap();
        let object = terms.term(s, l, q.object.as_ref()).unwrap();
        write.insert(graph, subject, predicate, &object, &[], &kind(&q.object));
    }
    write
}

/// Commits `writes` in one batch: each write's commit timestamp or refusal.
pub fn commit(
    store: &TestQuads,
    writes: Vec<QuadWrite<'_, SimStore, SimListCache<CachedList>>>,
) -> Vec<Result<u64, QuadError>> {
    let mut batch = store.lists().batch();
    let mut terms: Vec<WriteTerms> = Vec::new();
    for write in writes {
        let (request, t) = write.finish();
        batch.push(request);
        terms.push(t);
    }
    store.commit(batch, &terms).expect("the batch is written")
}

/// `quads` written and committed in namespace `ns`.
pub fn write(store: &TestQuads, ns: u64, quads: &[Quad]) -> u64 {
    let w = insert_write(store, ns, quads);
    commit(store, vec![w]).remove(0).expect("commits")
}

/// The UID of `iri` in namespace `ns`.
pub fn uid(store: &TestQuads, ns: u64, iri: &NamedNode) -> u64 {
    store.uid_of(ns, iri.as_str()).unwrap().expect("a UID")
}

/// The forward key of `subject`'s `predicate` in `graph`, all named by IRI.
pub fn forward(
    store: &TestQuads,
    ns: u64,
    graph: u64,
    subject: &NamedNode,
    predicate: &NamedNode,
) -> Vec<u8> {
    QuadKey::Forward {
        namespace: ns,
        graph,
        predicate: uid(store, ns, predicate),
        subject: uid(store, ns, subject),
    }
    .to_bytes()
}

/// The quad `s p o` in the default graph.
pub fn quad(s: &str, p: &str, o: impl Into<Term>) -> Quad {
    Quad::new(node(s), ex(p), o, GraphName::DefaultGraph)
}
