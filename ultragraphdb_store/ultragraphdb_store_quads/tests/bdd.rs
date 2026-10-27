/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The term-dictionary, quad-storage, contract-version and data-dictionary scenarios, on quads over the simulated
//! store.

use cucumber::{World, then, when};
use oxrdf::vocab::xsd;
use oxrdf::{GraphName, Literal, NamedNode, Quad, Term};
use ultragraphdb_core_keys::{Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_mapping::{CONTRACT_VERSION, ContractVersion};
use ultragraphdb_core_terms::{EncodedTerm, FLOAT32_VECTOR, TermError};
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_quads::{CONTRACT_KEY, QuadError, QuadErrorEnum, QuadStore, string_key};
use ultragraphdb_store_sim::{
    FaultConfig, OpRecord, Seed, SimClock, SimListCache, SimStore, Stream,
};
use ultragraphdb_traits_store::{PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

mod utils {
    #[path = "dictionary_workload.rs"]
    pub mod dictionary_workload;
    #[allow(dead_code)]
    #[path = "helpers.rs"]
    pub mod helpers;
}

mod dictionary_steps;

use crate::utils::helpers::{
    TestQuads, commit, ex, forward, insert_write, node, quad, quads, quads_over, uid, write,
};

#[derive(Debug, Default, World)]
struct Steps {
    store: Option<TestQuads>,
    uids: Vec<u64>,
    results: Vec<Result<u64, QuadError>>,
    ts: Vec<u64>,
    read: Vec<Term>,
    error: Option<QuadError>,
    log: Vec<OpRecord>,
    dictionary: Option<std::sync::Arc<ultragraphdb_store_quads::DataDictionary>>,
    flags: Vec<bool>,
}

fn plain_store() -> SimStore {
    SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new())
}

#[when("<a> is written in two separate commits")]
fn two_commits(w: &mut Steps) {
    let store = w.store.insert(quads());
    write(store, 0, &[quad("a", "knows", ex("b"))]);
    let first = uid(store, 0, &ex("a"));
    let second_write = insert_write(
        store,
        0,
        &[quad("a", "name", Literal::new_simple_literal("A"))],
    );
    assert!(
        !second_write.is_fresh(first),
        "the second write finds a in the dictionary"
    );
    commit(store, vec![second_write]).remove(0).unwrap();
    w.uids = vec![first, uid(store, 0, &ex("a"))];
}

#[then("both name the same UID")]
fn same_uid(w: &mut Steps) {
    assert_eq!(w.uids[0], w.uids[1]);
}

#[when("a UID is assigned to a new IRI")]
fn new_uid(w: &mut Steps) {
    let store = w.store.insert(quads());
    write(store, 0, &[quad("fresh", "p", ex("other"))]);
    w.uids = vec![uid(store, 0, &ex("fresh")), uid(store, 0, &ex("other"))];
}

#[then("its top 16 bits are zero")]
fn type_bits_zero(w: &mut Steps) {
    assert!(
        w.uids.iter().all(|u| *u >> 48 == 0 && *u != 0),
        "{:?}",
        w.uids
    );
}

#[when("two writes read before either commits, both create <new>, and both commit")]
fn two_creators(w: &mut Steps) {
    let store = w.store.insert(quads());
    let first = insert_write(store, 0, &[quad("new", "p", ex("x"))]);
    let second = insert_write(store, 0, &[quad("new", "q", ex("y"))]);
    w.results = commit(store, vec![first]);
    w.results.extend(commit(store, vec![second]));
}

#[then(expr = "the first commits, the second fails with {string}, and <new> has one UID")]
fn second_retries(w: &mut Steps, text: String) {
    assert!(w.results[0].is_ok());
    assert_eq!(w.results[1].as_ref().unwrap_err().to_string(), text);
    let store = w.store.as_ref().unwrap();
    let held = uid(store, 0, &ex("new"));
    let retry = insert_write(store, 0, &[quad("new", "q", ex("y"))]);
    assert!(
        !retry.is_fresh(held),
        "the retry finds the first write's UID"
    );
    commit(store, vec![retry]).remove(0).unwrap();
    assert_eq!(uid(store, 0, &ex("new")), held);
}

#[when("a write creating <gone> is refused at commit")]
fn refused_creator(w: &mut Steps) {
    let store = w.store.insert(quads());
    let claims = insert_write(store, 0, &[quad("x", "p", ex("y"))]);
    let refused = insert_write(store, 0, &[quad("x", "p", ex("gone"))]);
    w.results = commit(store, vec![claims, refused]);
}

#[then("the dictionary holds no entry for <gone>")]
fn no_gone(w: &mut Steps) {
    assert!(w.results[0].is_ok());
    assert!(matches!(
        w.results[1].as_ref().unwrap_err().0,
        QuadErrorEnum::Aborted
    ));
    assert_eq!(
        w.store
            .as_ref()
            .unwrap()
            .uid_of(0, ex("gone").as_str())
            .unwrap(),
        None
    );
}

#[when("<late> is created at 10 and a read at 5 lists every statement of the default graph")]
fn late_term(w: &mut Steps) {
    let store = w.store.insert(quads());
    let early = write(store, 0, &[quad("a", "p", ex("b"))]);
    write(
        store,
        0,
        &[quad("late", "p", ex("b")), quad("a", "p", ex("late"))],
    );
    let snapshot = store.store().snapshot(early).unwrap();
    let scan = Scan::prefix(key_prefix(Keyspace::Forward, false, 0, Some(0), None));
    let keys: Vec<Vec<u8>> = snapshot
        .scan(&scan)
        .map(|i| i.unwrap().key().to_vec())
        .collect();
    for key in keys {
        let QuadKey::Forward { subject, .. } = QuadKey::parse(&key).unwrap().0 else {
            unreachable!()
        };
        w.read
            .push(store.term_of(0, &EncodedTerm::uid(subject)).unwrap());
        w.read.extend(store.list_terms(0, &key, early).unwrap());
    }
}

#[then("the read returns no statement naming <late>")]
fn none_late(w: &mut Steps) {
    assert!(!w.read.is_empty());
    assert!(!w.read.contains(&ex("late").into()), "{:?}", w.read);
}

#[when("a list names a literal hash whose dictionary entry is missing and the list is read")]
fn dangling(w: &mut Steps) {
    let store = w.store.insert(quads());
    let long = Literal::new_simple_literal("a string long enough to be hashed");
    let ts = write(store, 0, &[quad("a", "note", long.clone())]);
    let encoded = EncodedTerm::literal(long.as_ref(), |_| Ok::<_, TermError>(0)).unwrap();
    let (hash, _) = encoded.strings().next().unwrap();
    let mut batch = WriteBatch::new();
    batch.remove_versionless(string_key(0, &hash));
    store.store().apply(batch, PersistMode::Buffer).unwrap();
    let key = forward(store, 0, 0, &ex("a"), &ex("note"));
    w.uids = key.iter().map(|b| u64::from(*b)).collect();
    w.error = store.list_terms(0, &key, ts).err();
}

#[then("the read fails with an error naming the list's key, and nothing panics")]
fn dangling_reported(w: &mut Steps) {
    let error = w.error.take().expect("refused");
    let hex: String = w.uids.iter().map(|b| format!("{b:02x}")).collect();
    assert!(error.to_string().contains(&hex), "{error}");
}

#[when("<a> <knows> <b> is written to the default graph")]
fn knows(w: &mut Steps) {
    let store = w.store.insert(quads());
    w.ts = vec![write(store, 0, &[quad("a", "knows", ex("b"))])];
}

#[then(
    "the forward list of knows for a holds b, the reverse list of knows for b holds a, the subject predicates of a hold knows, and the object predicates of b hold knows"
)]
fn four_lists(w: &mut Steps) {
    let store = w.store.as_ref().unwrap();
    let ts = w.ts[0];
    let (a, b, knows) = (
        uid(store, 0, &ex("a")),
        uid(store, 0, &ex("b")),
        uid(store, 0, &ex("knows")),
    );
    let terms = |key: QuadKey| store.list_terms(0, &key.to_bytes(), ts).unwrap();
    assert_eq!(
        terms(QuadKey::Forward {
            namespace: 0,
            graph: 0,
            predicate: knows,
            subject: a
        }),
        vec![Term::from(ex("b"))]
    );
    assert_eq!(
        terms(QuadKey::Reverse {
            namespace: 0,
            graph: 0,
            predicate: knows,
            object: EncodedTerm::uid(b)
        }),
        vec![Term::from(ex("a"))]
    );
    assert_eq!(
        terms(QuadKey::SubjectPredicates {
            namespace: 0,
            graph: 0,
            subject: a
        }),
        vec![Term::from(ex("knows"))]
    );
    assert_eq!(
        terms(QuadKey::ObjectPredicates {
            namespace: 0,
            graph: 0,
            object: EncodedTerm::uid(b)
        }),
        vec![Term::from(ex("knows"))]
    );
}

#[when("<a> <name> \"Ann\" and <c> <name> \"Ann\" are written")]
fn two_anns(w: &mut Steps) {
    let store = w.store.insert(quads());
    let ann = Literal::new_simple_literal("Ann");
    w.ts = vec![write(
        store,
        0,
        &[quad("a", "name", ann.clone()), quad("c", "name", ann)],
    )];
}

#[then("the reverse list of name for \"Ann\" holds a and c")]
fn reverse_ann(w: &mut Steps) {
    let store = w.store.as_ref().unwrap();
    let ann = EncodedTerm::literal(Literal::new_simple_literal("Ann").as_ref(), |_| {
        Ok::<_, TermError>(0)
    })
    .unwrap()
    .into_term();
    let key = QuadKey::Reverse {
        namespace: 0,
        graph: 0,
        predicate: uid(store, 0, &ex("name")),
        object: ann,
    };
    let mut held = store.list_terms(0, &key.to_bytes(), w.ts[0]).unwrap();
    held.sort_by_key(Term::to_string);
    assert_eq!(held, vec![Term::from(ex("a")), Term::from(ex("c"))]);
}

#[when(expr = "<a> <embedding> {string}^^ugdb:float32vector is written")]
fn vector_written(w: &mut Steps, lexical: String) {
    let store = w.store.insert(quads());
    let vector = Literal::new_typed_literal(lexical, NamedNode::new_unchecked(FLOAT32_VECTOR));
    w.read = vec![vector.clone().into()];
    w.ts = vec![write(store, 0, &[quad("a", "embedding", vector)])];
}

#[then(
    "the forward list of embedding for a holds the vector, and no reverse or object-predicate key names it"
)]
fn vector_forward_only(w: &mut Steps) {
    let store = w.store.as_ref().unwrap();
    let key = forward(store, 0, 0, &ex("a"), &ex("embedding"));
    assert_eq!(store.list_terms(0, &key, w.ts[0]).unwrap(), w.read);
    let snapshot = store.store().snapshot(u64::MAX).unwrap();
    for keyspace in [Keyspace::Reverse, Keyspace::ObjectPredicates] {
        let scan = Scan::prefix(key_prefix(keyspace, false, 0, None, None));
        assert_eq!(snapshot.scan(&scan).count(), 0, "{keyspace:?}");
    }
}

#[when("<a> <p> <b> is written to graph <g1> and read from graph <g2>")]
fn graphs(w: &mut Steps) {
    let store = w.store.insert(quads());
    let ts = write(
        store,
        0,
        &[Quad::new(
            node("a"),
            ex("p"),
            ex("b"),
            GraphName::NamedNode(ex("g1")),
        )],
    );
    write(
        store,
        0,
        &[Quad::new(
            node("x"),
            ex("p"),
            ex("y"),
            GraphName::NamedNode(ex("g2")),
        )],
    );
    let key = forward(store, 0, uid(store, 0, &ex("g2")), &ex("a"), &ex("p"));
    w.read = store.list_terms(0, &key, ts + 1).unwrap();
    let held = forward(store, 0, uid(store, 0, &ex("g1")), &ex("a"), &ex("p"));
    assert_eq!(
        store.list_terms(0, &held, ts + 1).unwrap(),
        vec![Term::from(ex("b"))]
    );
}

#[then("the read returns nothing")]
fn nothing(w: &mut Steps) {
    assert!(w.read.is_empty());
}

#[when("100 distinct statements of knows are written to the default graph")]
fn hundred(w: &mut Steps) {
    let store = w.store.insert(quads());
    let first: Vec<Quad> = (0..60)
        .map(|i| quad("a", "knows", ex(&format!("b{i}"))))
        .collect();
    let second: Vec<Quad> = (60..100)
        .map(|i| quad("c", "knows", ex(&format!("b{i}"))))
        .collect();
    write(store, 0, &first);
    w.ts = vec![write(store, 0, &second)];
}

#[then("the statistics of knows report 100 statements")]
fn hundred_counted(w: &mut Steps) {
    let store = w.store.as_ref().unwrap();
    let stats = store
        .statistics(0, 0, uid(store, 0, &ex("knows")), w.ts[0])
        .unwrap();
    assert_eq!(stats.statements(), 100);
    assert_eq!(stats.kind("iri"), 100);
    assert!(
        (80..=120).contains(&stats.distinct_objects()),
        "{}",
        stats.distinct_objects()
    );
}

#[when("<a> <age> 30 and <b> <age> \"unknown\" are written")]
fn ages(w: &mut Steps) {
    let store = w.store.insert(quads());
    w.ts = vec![write(
        store,
        0,
        &[
            quad("a", "age", Literal::from(30)),
            quad("b", "age", Literal::new_simple_literal("unknown")),
        ],
    )];
}

#[then("the statistics of age report one xsd:integer and one xsd:string literal")]
fn ages_counted(w: &mut Steps) {
    let store = w.store.as_ref().unwrap();
    let stats = store
        .statistics(0, 0, uid(store, 0, &ex("age")), w.ts[0])
        .unwrap();
    assert_eq!(
        (
            stats.kind(xsd::INTEGER.as_str()),
            stats.kind(xsd::STRING.as_str())
        ),
        (1, 1)
    );
}

#[when("a reverse list grows past the split threshold and is rolled up")]
fn big_reverse(w: &mut Steps) {
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    let lists: Lists<SimStore, SimListCache<CachedList>> = Lists::new(
        plain_store(),
        cache,
        ListsConfig::new(PersistMode::Buffer, 2_000),
    );
    let store = w.store.insert(QuadStore::open(lists).unwrap());
    let quads: Vec<Quad> = (0..3_000)
        .map(|i| quad(&format!("s{i}"), "knows", ex("hub")))
        .collect();
    let ts = write(store, 0, &quads);
    let key = QuadKey::Reverse {
        namespace: 0,
        graph: 0,
        predicate: uid(store, 0, &ex("knows")),
        object: EncodedTerm::uid(uid(store, 0, &ex("hub"))),
    }
    .to_bytes();
    store.lists().rollup_key(&key).unwrap();
    w.ts = vec![ts];
    w.uids = key.iter().map(|b| u64::from(*b)).collect();
}

#[then("it is stored as parts, and reading it returns every subject in order")]
fn reverse_split(w: &mut Steps) {
    let store = w.store.as_ref().unwrap();
    let key: Vec<u8> = w.uids.iter().map(|b| *b as u8).collect();
    let list = store.lists().read(&key, u64::MAX).unwrap();
    assert!(!list.plist().splits().is_empty(), "the list is split");
    let subjects: Vec<u64> = store
        .objects(&key, u64::MAX)
        .unwrap()
        .iter()
        .map(|(o, _)| o.as_uid().unwrap())
        .collect();
    assert_eq!(subjects.len(), 3_000);
    assert!(subjects.windows(2).all(|p| p[0] < p[1]));
}

#[when("a write creates <new> and writes <new> <knows> <b>")]
fn fresh_subject(w: &mut Steps) {
    let sim = plain_store();
    let store = w.store.insert(quads_over(sim.clone()));
    write(store, 0, &[quad("b", "knows", ex("c"))]);
    sim.take_log();
    let ts = write(store, 0, &[quad("new", "knows", ex("b"))]);
    w.log = sim.take_log();
    w.ts = vec![ts];
}

#[then("the write reads no forward, subject-predicate or statement key of <new>")]
fn fresh_unread(w: &mut Steps) {
    let new = uid(w.store.as_ref().unwrap(), 0, &ex("new"));
    let names_new = |key: &[u8]| match QuadKey::parse(key) {
        Ok((QuadKey::Forward { subject, .. }, _))
        | Ok((QuadKey::SubjectPredicates { subject, .. }, _)) => subject == new,
        Ok((QuadKey::Statement { .. }, _)) => true,
        _ => false,
    };
    for record in &w.log {
        let keys: Vec<&[u8]> = match record {
            OpRecord::Read { key, .. } => vec![key],
            OpRecord::ReadMany { keys, .. } => keys.iter().map(Vec::as_slice).collect(),
            OpRecord::Scanned { scan, .. } => vec![scan.start()],
            _ => vec![],
        };
        assert!(!keys.iter().any(|k| names_new(k)), "read {record:?}");
    }
}

#[when("a store written under contract version 2 is opened by a build implementing version 1")]
fn version_2(w: &mut Steps) {
    let sim = plain_store();
    let mut batch = WriteBatch::new();
    batch.put_versionless(
        CONTRACT_KEY.to_vec(),
        ContractVersion::new(2, 0).to_bytes().to_vec(),
    );
    sim.apply(batch, PersistMode::Sync).unwrap();
    let before = sim.contents();
    let cache = SimListCache::new(16, Seed::new(1).stream(Stream::Cache));
    w.error = QuadStore::open(Lists::new(sim.clone(), cache, ListsConfig::default())).err();
    assert_eq!(sim.contents(), before, "nothing is written");
    assert_eq!(CONTRACT_VERSION.major(), 1);
}

#[then("opening fails with an error naming contract versions 2 and 1, and nothing is written")]
fn version_refused(w: &mut Steps) {
    let text = w.error.take().expect("refused").to_string();
    assert!(
        text.contains("version 2.0") && text.contains("version 1.0"),
        "{text}"
    );
}

#[when("a seeded workload creates terms and writes quads naming them under every fault class")]
fn dictionary_workload(w: &mut Steps) {
    for seed in 300..308 {
        let (_, checked) = crate::utils::dictionary_workload::DictionaryWorkload::run(
            Seed::new(seed),
            crate::utils::dictionary_workload::DictionaryWorkload::faults(),
            150,
        )
        .unwrap_or_else(|v| panic!("{v}"));
        w.uids.push(checked as u64);
    }
}

#[then(
    "every stored UID and literal hash has its dictionary entry after every step and every recovery"
)]
fn dictionary_complete(w: &mut Steps) {
    assert!(
        w.uids.iter().sum::<u64>() > 200,
        "{:?} lists checked",
        w.uids
    );
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()),
    );
}
