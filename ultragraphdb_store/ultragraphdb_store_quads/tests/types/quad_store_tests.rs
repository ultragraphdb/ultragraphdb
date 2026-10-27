/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{BlankNode, GraphName, Literal, Quad, Term};
use ultragraphdb_core_keys::QuadKey;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_quads::QuadStore;
use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimListCache, SimStore, Stream};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

use crate::utils::helpers::{ex, forward, quad, quads, uid, write};

fn lists(store: SimStore) -> Lists<SimStore, SimListCache<CachedList>> {
    Lists::new(
        store,
        SimListCache::new(16, Seed::new(1).stream(Stream::Cache)),
        ListsConfig::default(),
    )
}

#[test]
fn test_a_new_store_records_the_contract_and_a_store_with_data_but_none_is_refused() {
    let store = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    QuadStore::open(lists(store.clone())).unwrap();
    QuadStore::open(lists(store)).expect("its own version opens again");
    let old = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(b"\x00stage-2 key".to_vec(), 1, 4, vec![]));
    old.apply(batch, PersistMode::Sync).unwrap();
    let error = QuadStore::open(lists(old)).unwrap_err();
    assert!(error.to_string().contains("version 0.0"), "{error}");
}

#[test]
fn test_a_blank_node_is_skolemized_once_per_write_and_presented_as_a_blank_node() {
    let store = quads();
    let x = BlankNode::new_unchecked("x");
    let ts = write(
        &store,
        0,
        &[
            Quad::new(x.clone(), ex("knows"), ex("b"), GraphName::DefaultGraph),
            Quad::new(ex("c"), ex("knows"), x.clone(), GraphName::DefaultGraph),
        ],
    );
    write(
        &store,
        0,
        &[Quad::new(x, ex("knows"), ex("d"), GraphName::DefaultGraph)],
    );
    let reverse = |o: &str| QuadKey::Reverse {
        namespace: 0,
        graph: 0,
        predicate: uid(&store, 0, &ex("knows")),
        object: EncodedTerm::uid(uid(&store, 0, &ex(o))),
    };
    let subjects_of_b = store
        .list_terms(0, &reverse("b").to_bytes(), u64::MAX)
        .unwrap();
    let targets_of_c = store
        .list_terms(0, &forward(&store, 0, 0, &ex("c"), &ex("knows")), ts)
        .unwrap();
    assert!(matches!(subjects_of_b[0], Term::BlankNode(_)));
    assert_eq!(
        subjects_of_b, targets_of_c,
        "one blank node for every statement of the write naming _:x"
    );
    let subjects_of_d = store
        .list_terms(0, &reverse("d").to_bytes(), u64::MAX)
        .unwrap();
    assert_ne!(
        subjects_of_b, subjects_of_d,
        "another write's _:x is another node"
    );
    let (_, skolem) = store
        .iri_of(0, subjects_of_b[0].to_string()[3..].parse().unwrap())
        .unwrap();
    assert!(skolem);
}

#[test]
fn test_literals_of_every_kind_read_back_exactly() {
    let store = quads();
    let objects: Vec<Term> = vec![
        Literal::new_typed_literal("01", oxrdf::vocab::xsd::INTEGER).into(),
        Literal::new_typed_literal("twelve", oxrdf::vocab::xsd::INTEGER).into(),
        Literal::new_language_tagged_literal("a long label in English", "en")
            .unwrap()
            .into(),
        Literal::new_typed_literal("x", ex("custom")).into(),
        Literal::new_simple_literal("a string longer than fifteen bytes").into(),
    ];
    let quads: Vec<Quad> = objects.iter().map(|o| quad("a", "v", o.clone())).collect();
    let ts = write(&store, 0, &quads);
    let mut read = store
        .list_terms(0, &forward(&store, 0, 0, &ex("a"), &ex("v")), ts)
        .unwrap();
    let mut expected = objects;
    read.sort_by_key(Term::to_string);
    expected.sort_by_key(Term::to_string);
    assert_eq!(read, expected);
}

/// A write that tested a statement in a list a refused write of its batch wrote is refused too: its test read that
/// write's delta, which is never committed.
#[test]
fn test_a_write_testing_a_list_a_refused_write_wrote_is_refused() {
    use ultragraphdb_core_terms::EncodedTerm;
    use ultragraphdb_store_quads::{QuadErrorEnum, QuadWrite};

    let store = quads();
    write(
        &store,
        0,
        &[quad("a", "knows", ex("c")), quad("b", "p", ex("q"))],
    );
    let ts = store.lists().applied_ts();
    let (a, knows, b) = (
        uid(&store, 0, &ex("a")),
        uid(&store, 0, &ex("knows")),
        EncodedTerm::uid(uid(&store, 0, &ex("b"))),
    );
    let mut batch = store.lists().batch();
    let mut first = QuadWrite::new(&store, 0, batch.request(ts));
    let new = {
        let (terms, quads) = first.terms();
        terms
            .iri(quads.store(), quads.leases(), ex("n").as_str())
            .unwrap()
    };
    first.insert(0, a, knows, &b, &[], "iri");
    first.insert(0, new, knows, &b, &[], "iri");
    let (request, first_terms) = first.finish();
    batch.push(request);
    let mut second = QuadWrite::new(&store, 0, batch.request(ts));
    assert!(
        second.holds(0, a, knows, &b).unwrap().is_some(),
        "it reads the first write's delta"
    );
    let (request, second_terms) = second.finish();
    batch.push(request);
    // A commit claims <n> after the first write took it as new.
    write(&store, 0, &[quad("n", "p", ex("q"))]);
    let results = store.commit(batch, &[first_terms, second_terms]).unwrap();
    assert!(matches!(&results[0], Err(e) if matches!(e.0, QuadErrorEnum::Aborted)));
    assert!(matches!(&results[1], Err(e) if matches!(e.0, QuadErrorEnum::Aborted)));
}
