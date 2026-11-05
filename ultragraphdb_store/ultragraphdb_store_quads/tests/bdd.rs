/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The term-dictionary, quad-storage, contract-version and data-dictionary scenarios, on quads over the simulated
//! store.

use cucumber::{World, then, when};
use oxrdf::vocab::xsd;
use oxrdf::{GraphName, Literal, NamedNode, Quad, Term};
use ultragraphdb_core_ids::{Bucket, SystemId, TypePrefix};
use ultragraphdb_core_keys::{Graph, Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_mapping::{CONTRACT_VERSION, ContractVersion, RandomSource};
use ultragraphdb_core_terms::{EncodedTerm, FLOAT32_VECTOR, TermError};
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_quads::{
    CONTRACT_KEY, OwnedBuckets, QuadError, QuadStore, WriteTerms, literal_key, tenant_base,
};
use ultragraphdb_store_sim::{OpRecord, Seed, SimListCache, SimStore, Stream};
use ultragraphdb_traits_store::{PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

mod utils {
    #[path = "dictionary_workload.rs"]
    pub mod dictionary_workload;
    #[path = "helpers.rs"]
    pub mod helpers;
}

mod dictionary_steps;

use crate::utils::helpers::{
    TestQuads, blocks, commit, ex, forward, id, insert_write, named, node, quad, quads, quads_over,
    quads_with, sim, write, write_at,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

#[derive(Debug, Default, World)]
struct Steps {
    store: Option<TestQuads>,
    ids: Vec<SystemId>,
    texts: Vec<String>,
    ts: Vec<u64>,
    read: Vec<Term>,
    key: Vec<u8>,
    error: Option<QuadError>,
    log: Vec<OpRecord>,
    dictionary: Option<std::sync::Arc<ultragraphdb_store_quads::DataDictionary>>,
    flags: Vec<bool>,
}

/// The first bucket of tenant 0's run: the top byte of the xxHash reference's XXH3-64 of eight zero bytes, 0xC7
/// (core_ids' `testdata/golden/buckets.json`).
const RUN_0: u32 = 0xC7_0000;

/// Every statement of the default graph naming `term` at `read_ts`: the subjects and objects of its forward lists.
fn statements_naming(store: &TestQuads, term: &Term, read_ts: u64) -> TestResult<Vec<Term>> {
    let snapshot = store.store().snapshot(read_ts)?;
    let scan = Scan::prefix(key_prefix(
        Keyspace::Forward,
        false,
        0,
        Some(Graph::Default),
        None,
    ));
    let keys: Vec<Vec<u8>> = snapshot
        .scan(&scan)
        .map(|i| i.map(|i| i.key().to_vec()))
        .collect::<Result<_, _>>()?;
    let mut found = Vec::new();
    for key in keys {
        let QuadKey::Forward { subject, .. } = QuadKey::parse(&key)?.0 else {
            return Err(TestError::new("a forward key parses as a forward key"));
        };
        let subject = store.term_of(0, &EncodedTerm::id(subject))?;
        for object in store.list_terms(0, &key, read_ts)? {
            if subject == *term || object == *term {
                found.push(subject.clone());
                found.push(object);
            }
        }
    }
    Ok(found)
}

#[when(
    "a write creating `<gone>` is refused at commit, and a read then asks for every statement naming `<gone>`"
)]
fn refused_creator(w: &mut Steps) -> TestResult {
    let store = w.store.insert(quads()?);
    write(store, 0, &[quad("x", "p", ex("y"))])?;
    let refused = insert_write(
        store,
        0,
        &[quad("x", "p", ex("gone")), quad("gone", "p", ex("y"))],
    )?;
    // Refused by the commit step: its request is never written, and its terms are released.
    let (_, terms) = refused.finish();
    store.release(&terms);
    w.flags = vec![store.id_of(0, ex("gone").as_str())?.is_some()];
    w.read = statements_naming(store, &ex("gone").into(), store.lists().applied_ts())?;
    Ok(())
}

#[then("the read returns nothing, whether or not the dictionary holds an entry for `<gone>`")]
fn nothing_gone(w: &mut Steps) {
    assert_eq!(w.flags, vec![false], "the released write wrote no entry");
    assert!(w.read.is_empty(), "{:?}", w.read);
}

#[when(
    "`<late>` is first written at epoch 10 and a read at epoch 5 lists every statement of the default graph"
)]
fn late_term(w: &mut Steps) -> TestResult {
    // Commit versions stand for the epochs here: the read is at the version before `<late>` was first written.
    let store = w.store.insert(quads()?);
    let early = write_at(store, 0, &[quad("a", "p", ex("b"))])?;
    write(
        store,
        0,
        &[quad("late", "p", ex("b")), quad("a", "p", ex("late"))],
    )?;
    w.read = statements_naming(store, &ex("a").into(), early)?;
    w.read
        .extend(statements_naming(store, &ex("late").into(), early)?);
    Ok(())
}

#[then("the read returns no statement naming `<late>`")]
fn none_late(w: &mut Steps) {
    assert!(!w.read.is_empty());
    assert!(!w.read.contains(&ex("late").into()), "{:?}", w.read);
}

#[when("a list names a literal hash whose dictionary entry is missing and the list is read")]
fn dangling(w: &mut Steps) -> TestResult {
    let store = w.store.insert(quads()?);
    let long = Literal::new_simple_literal("a string long enough to be hashed");
    let ts = write_at(store, 0, &[quad("a", "note", long.clone())])?;
    let encoded = EncodedTerm::literal(long.as_ref(), |_| {
        Err(TermError::malformed(
            Vec::new(),
            "a simple literal names no datatype",
        ))
    })?;
    let (hash, _) = encoded.strings().next().context("a hashed string")?;
    let mut batch = WriteBatch::new();
    batch.remove_versionless(literal_key(0, &hash));
    store.store().apply(batch, PersistMode::Buffer)?;
    w.key = forward(store, 0, Graph::Default, &ex("a"), &ex("note"))?;
    w.error = store.list_terms(0, &w.key, ts).err();
    Ok(())
}

#[then("the read fails with an error naming the list's key, and nothing panics")]
fn dangling_reported(w: &mut Steps) -> TestResult {
    let error = w.error.take().context("refused")?;
    let hex: String = w.key.iter().map(|b| format!("{b:02x}")).collect();
    assert!(error.to_string().contains(&hex), "{error}");
    Ok(())
}

#[when(
    "partition 1 holds `<a> <knows> <b>`, `<b>` homed in partition 2, and reads `<a>`'s `knows` while partition 2 is unreachable"
)]
fn foreign_reference(w: &mut Steps) -> TestResult {
    let source = blocks();
    let partition_2 = quads_with(sim(), source.clone(), 2)?;
    write(
        &partition_2,
        0,
        &[quad("b", "name", Literal::new_simple_literal("B"))],
    )?;
    let b = id(&partition_2, 0, &ex("b"))?;
    let partition_1 = w.store.insert(quads_with(sim(), source, 1)?);
    // The registration's reply named `<b>`'s ID; the commit copies its entries into partition 1's dictionary.
    let mut writing = insert_write(partition_1, 0, &[])?;
    let (a, knows) = {
        let (terms, q) = writing.terms();
        terms.copy(ex("b").as_str(), b);
        (
            terms.iri(q, ex("a").as_str())?,
            terms.iri(q, ex("knows").as_str())?,
        )
    };
    writing.insert(Graph::Default, a, knows, &EncodedTerm::id(b), &[], "iri");
    let ts = commit(partition_1, vec![writing])?
        .first()
        .copied()
        .context("committed")?;
    drop(partition_2);
    let key = forward(partition_1, 0, Graph::Default, &ex("a"), &ex("knows"))?;
    w.read = partition_1.list_terms(0, &key, ts)?;
    w.ids = vec![b];
    Ok(())
}

#[then("the read names `<b>` by its IRI from partition 1's dictionary")]
fn named_locally(w: &mut Steps) -> TestResult {
    assert_eq!(w.read, vec![Term::from(ex("b"))]);
    let store = w.store.as_ref().context("partition 1")?;
    assert_eq!(store.id_of(0, ex("b").as_str())?, w.ids.first().copied());
    Ok(())
}

#[when("`<a>` is written in two separate commits")]
fn two_commits(w: &mut Steps) -> TestResult {
    let store = w.store.insert(quads()?);
    write(store, 0, &[quad("a", "knows", ex("b"))])?;
    let first = id(store, 0, &ex("a"))?;
    let second_write = insert_write(
        store,
        0,
        &[quad("a", "name", Literal::new_simple_literal("A"))],
    )?;
    assert!(
        !second_write.is_fresh(first),
        "the second write finds a in the dictionary"
    );
    let _versions: Vec<u64> = commit(store, vec![second_write])?;
    w.ids = vec![first, id(store, 0, &ex("a"))?];
    Ok(())
}

#[then("both name the same system ID")]
fn same_id(w: &mut Steps) {
    assert_eq!(w.ids.first(), w.ids.get(1));
    assert_eq!(w.ids.len(), 2);
}

#[when("a new IRI receives a system ID")]
fn new_id(w: &mut Steps) -> TestResult {
    let store = w.store.insert(quads()?);
    write(store, 0, &[quad("a", "p", ex("other"))])?;
    w.ids = vec![id(store, 0, &ex("a"))?];
    Ok(())
}

#[then(
    "its type prefix is 1 and its bucket equals the bucket computed from its tenant and canonical encoding"
)]
fn own_bucket(w: &mut Steps) -> TestResult {
    let a = w.ids.first().context("an ID")?;
    assert_eq!(a.type_prefix().get(), 1);
    // core_ids' `testdata/golden/buckets.json`, "the scenario: tenant 0": http://ex.org/a in tenant 0, from the
    // xxHash reference.
    assert_eq!(a.bucket().get(), 0xC7_F1AB);
    Ok(())
}

#[when(
    "partition 1, owning the first 21,846 buckets of tenant 0's run, mints a blank node with its random source seeded to a recorded value"
)]
fn mint_blank(w: &mut Steps) -> TestResult {
    let owned = OwnedBuckets::new(&[(Bucket::new(RUN_0)?, Bucket::new(RUN_0 + 21_845)?)])
        .context("ranges")?;
    for _ in 0..2 {
        let store = quads_with(sim(), blocks(), 0)?.owning(owned.clone());
        let mut terms = WriteTerms::new(0);
        let blank = terms.blank(&store, "x")?;
        let (skolem, is_blank) = terms.pending_iri(blank).context("its skolem IRI")?;
        assert!(is_blank);
        w.texts.push(skolem.to_string());
        w.ids.push(blank);
    }
    Ok(())
}

#[then(
    "the skolem IRI is the tenant's skolem base, `.well-known/genid/`, 6 hexadecimal digits naming one of those buckets, `-` and 26 hexadecimal digits; the node's ID carries that bucket; and the same seed mints the same IRI"
)]
fn skolem_names_bucket(w: &mut Steps) -> TestResult {
    let skolem = w.texts.first().context("a skolem IRI")?;
    let tail = skolem
        .strip_prefix(&format!("{}.well-known/genid/", tenant_base(0)))
        .context("the skolem base and the genid path")?;
    let (bucket, random) = tail.split_once('-').context("a bucket and random digits")?;
    assert_eq!((bucket.len(), random.len()), (6, 26), "{skolem}");
    let hex = |s: &str| {
        s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    assert!(hex(bucket) && hex(random), "{skolem}");
    let bucket = u32::from_str_radix(bucket, 16)?;
    assert!((RUN_0..RUN_0 + 21_846).contains(&bucket), "{bucket:#x}");
    // Seeded 0, the recorded value: the bucket and bits SplitMix64's first three outputs give (see the unit test).
    assert_eq!(tail, "c74b60-6aa1b965f406c45d188009454f");
    let blank = w.ids.first().context("an ID")?;
    assert_eq!(blank.bucket().get(), bucket);
    assert_eq!(blank.type_prefix(), TypePrefix::RESOURCE);
    assert_eq!(
        w.texts.get(1),
        Some(skolem),
        "the same seed mints the same IRI"
    );
    Ok(())
}

#[when("a transaction creates `<new>` and aborts, and a later transaction writes `<new>`")]
fn aborted_creator(w: &mut Steps) -> TestResult {
    let store = w.store.insert(quads()?);
    let aborted = insert_write(
        store,
        0,
        &[quad("new", "p", Literal::new_simple_literal("x"))],
    )?;
    let (_, terms) = aborted.finish();
    let created = terms
        .written()
        .find(|(t, _)| *t == ex("new").as_str())
        .map(|(_, id)| id)
        .context("created")?;
    drop(terms);
    w.ts = vec![write_at(
        store,
        0,
        &[quad("new", "p", Literal::new_simple_literal("x"))],
    )?];
    w.ids = vec![created];
    Ok(())
}

#[then("the later transaction commits, and `<new>` has one system ID")]
fn new_usable(w: &mut Steps) -> TestResult {
    let store = w.store.as_ref().context("the store")?;
    assert!(w.ts.first().is_some_and(|ts| *ts > 0));
    let held = id(store, 0, &ex("new"))?;
    assert_eq!(
        Some(held),
        w.ids.first().copied(),
        "the aborted creator's ID"
    );
    assert_eq!(
        store.iri_of(0, held)?,
        (ex("new").as_str().to_string(), false)
    );
    Ok(())
}

#[when("`<a> <knows> <b>` is written to the default graph")]
fn knows(w: &mut Steps) -> TestResult {
    let store = w.store.insert(quads()?);
    w.ts = vec![write_at(store, 0, &[quad("a", "knows", ex("b"))])?];
    Ok(())
}

#[then(
    "the forward list of `knows` for `a` holds `b`, the reverse list of `knows` for `b` holds `a`, the subject predicates of `a` hold `knows`, and the object predicates of `b` hold `knows`, all in the partition owning `a`"
)]
fn four_lists(w: &mut Steps) -> TestResult {
    // One store is the partition owning `a`.
    let store = w.store.as_ref().context("the store")?;
    let ts = *w.ts.first().context("a version")?;
    let (a, b, knows) = (
        id(store, 0, &ex("a"))?,
        id(store, 0, &ex("b"))?,
        id(store, 0, &ex("knows"))?,
    );
    let terms = |key: QuadKey| store.list_terms(0, &key.to_bytes(), ts);
    let d = Graph::Default;
    assert_eq!(
        terms(QuadKey::Forward {
            tenant: 0,
            graph: d,
            predicate: knows,
            subject: a
        })?,
        vec![Term::from(ex("b"))]
    );
    assert_eq!(
        terms(QuadKey::Reverse {
            tenant: 0,
            graph: d,
            predicate: knows,
            object: EncodedTerm::id(b)
        })?,
        vec![Term::from(ex("a"))]
    );
    assert_eq!(
        terms(QuadKey::SubjectPredicates {
            tenant: 0,
            graph: d,
            subject: a
        })?,
        vec![Term::from(ex("knows"))]
    );
    assert_eq!(
        terms(QuadKey::ObjectPredicates {
            tenant: 0,
            graph: d,
            object: EncodedTerm::id(b)
        })?,
        vec![Term::from(ex("knows"))]
    );
    Ok(())
}

#[when("`<a> <name> \"Ann\"` and `<c> <name> \"Ann\"` are written, `a` and `c` in one partition")]
fn two_anns(w: &mut Steps) -> TestResult {
    let store = w.store.insert(quads()?);
    let ann = Literal::new_simple_literal("Ann");
    w.ts = vec![write_at(
        store,
        0,
        &[quad("a", "name", ann.clone()), quad("c", "name", ann)],
    )?];
    Ok(())
}

#[then("that partition's reverse list of `name` for `\"Ann\"` holds `a` and `c`")]
fn reverse_ann(w: &mut Steps) -> TestResult {
    let store = w.store.as_ref().context("the store")?;
    let ann = EncodedTerm::literal(Literal::new_simple_literal("Ann").as_ref(), |_| {
        Err(TermError::malformed(
            Vec::new(),
            "a simple literal names no datatype",
        ))
    })?
    .into_term();
    let key = QuadKey::Reverse {
        tenant: 0,
        graph: Graph::Default,
        predicate: id(store, 0, &ex("name"))?,
        object: ann,
    };
    let mut held = store.list_terms(0, &key.to_bytes(), *w.ts.first().context("a version")?)?;
    held.sort_by_key(Term::to_string);
    assert_eq!(held, vec![Term::from(ex("a")), Term::from(ex("c"))]);
    Ok(())
}

#[when(expr = "`<a> <embedding> {string}^^ugdb:float32vector` is written")]
fn vector_written(w: &mut Steps, lexical: String) -> TestResult {
    let store = w.store.insert(quads()?);
    let vector = Literal::new_typed_literal(lexical, NamedNode::new_unchecked(FLOAT32_VECTOR));
    w.read = vec![vector.clone().into()];
    w.ts = vec![write_at(store, 0, &[quad("a", "embedding", vector)])?];
    Ok(())
}

#[then(
    "the forward list of `embedding` for `a` holds the vector, and no reverse or object-predicate key names it"
)]
fn vector_forward_only(w: &mut Steps) -> TestResult {
    let store = w.store.as_ref().context("the store")?;
    let key = forward(store, 0, Graph::Default, &ex("a"), &ex("embedding"))?;
    assert_eq!(
        store.list_terms(0, &key, *w.ts.first().context("a version")?)?,
        w.read
    );
    let snapshot = store.store().snapshot(u64::MAX)?;
    for keyspace in [Keyspace::Reverse, Keyspace::ObjectPredicates] {
        let scan = Scan::prefix(key_prefix(keyspace, false, 0, None, None));
        assert_eq!(snapshot.scan(&scan).count(), 0, "{keyspace:?}");
    }
    Ok(())
}

#[when("`<a> <p> <b>` is written to graph `<g1>` and read from graph `<g2>`")]
fn graphs(w: &mut Steps) -> TestResult {
    let store = w.store.insert(quads()?);
    let ts = write_at(
        store,
        0,
        &[Quad::new(
            node("a"),
            ex("p"),
            ex("b"),
            GraphName::NamedNode(ex("g1")),
        )],
    )?;
    write(
        store,
        0,
        &[Quad::new(
            node("x"),
            ex("p"),
            ex("y"),
            GraphName::NamedNode(ex("g2")),
        )],
    )?;
    let key = forward(store, 0, named(store, 0, &ex("g2"))?, &ex("a"), &ex("p"))?;
    w.read = store.list_terms(0, &key, ts + 2)?;
    let held = forward(store, 0, named(store, 0, &ex("g1"))?, &ex("a"), &ex("p"))?;
    assert_eq!(
        store.list_terms(0, &held, ts + 2)?,
        vec![Term::from(ex("b"))]
    );
    Ok(())
}

#[then("the read returns nothing")]
fn nothing(w: &mut Steps) {
    assert!(w.read.is_empty());
}

#[when("100 distinct statements of knows are written to the default graph")]
fn hundred(w: &mut Steps) -> TestResult {
    let store = w.store.insert(quads()?);
    let first: Vec<Quad> = (0..60)
        .map(|i| quad("a", "knows", ex(&format!("b{i}"))))
        .collect();
    let second: Vec<Quad> = (60..100)
        .map(|i| quad("c", "knows", ex(&format!("b{i}"))))
        .collect();
    write(store, 0, &first)?;
    w.ts = vec![write_at(store, 0, &second)?];
    Ok(())
}

#[then("the statistics of knows report 100 statements")]
fn hundred_counted(w: &mut Steps) -> TestResult {
    let store = w.store.as_ref().context("the store")?;
    let stats = store.statistics(
        0,
        Graph::Default,
        id(store, 0, &ex("knows"))?,
        *w.ts.first().context("a version")?,
    )?;
    assert_eq!(stats.statements(), 100);
    assert_eq!(stats.kind("iri"), 100);
    assert!(
        (80..=120).contains(&stats.distinct_objects()),
        "{}",
        stats.distinct_objects()
    );
    Ok(())
}

#[when("<a> <age> 30 and <b> <age> \"unknown\" are written")]
fn ages(w: &mut Steps) -> TestResult {
    let store = w.store.insert(quads()?);
    w.ts = vec![write_at(
        store,
        0,
        &[
            quad("a", "age", Literal::from(30)),
            quad("b", "age", Literal::new_simple_literal("unknown")),
        ],
    )?];
    Ok(())
}

#[then("the statistics of age report one xsd:integer and one xsd:string literal")]
fn ages_counted(w: &mut Steps) -> TestResult {
    let store = w.store.as_ref().context("the store")?;
    let stats = store.statistics(
        0,
        Graph::Default,
        id(store, 0, &ex("age"))?,
        *w.ts.first().context("a version")?,
    )?;
    assert_eq!(
        (
            stats.kind(xsd::INTEGER.as_str()),
            stats.kind(xsd::STRING.as_str())
        ),
        (1, 1)
    );
    Ok(())
}

#[when("a reverse list grows past the split threshold and is rolled up")]
fn big_reverse(w: &mut Steps) -> TestResult {
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    let lists: Lists<SimStore, SimListCache<CachedList>> =
        Lists::new(sim(), cache, ListsConfig::new(PersistMode::Buffer, 2_000));
    let store = w
        .store
        .insert(QuadStore::open(lists, blocks(), RandomSource::new(3))?);
    let quads: Vec<Quad> = (0..3_000)
        .map(|i| quad(&format!("s{i}"), "knows", ex("hub")))
        .collect();
    let ts = write_at(store, 0, &quads)?;
    let key = QuadKey::Reverse {
        tenant: 0,
        graph: Graph::Default,
        predicate: id(store, 0, &ex("knows"))?,
        object: EncodedTerm::id(id(store, 0, &ex("hub"))?),
    }
    .to_bytes();
    let _rollup_key: bool = store.lists().rollup_key(&key)?;
    w.ts = vec![ts];
    w.key = key;
    Ok(())
}

#[then("it is stored as parts, and reading it returns every subject in order")]
fn reverse_split(w: &mut Steps) -> TestResult {
    let store = w.store.as_ref().context("the store")?;
    let key = w.key.clone();
    let list = store.lists().read(&key, u64::MAX)?;
    assert!(!list.plist().splits().is_empty(), "the list is split");
    let subjects: Vec<SystemId> = store
        .objects(&key, u64::MAX)?
        .iter()
        .map(|(o, _)| o.as_id().context("an object ID"))
        .collect::<TestResult<_>>()?;
    assert_eq!(subjects.len(), 3_000);
    assert!(subjects.windows(2).all(|p| p[0] < p[1]));
    Ok(())
}

#[when("a write creates `<new>` and writes `<new> <knows> <b>`")]
fn fresh_subject(w: &mut Steps) -> TestResult {
    let store_sim = sim();
    let store = w.store.insert(quads_over(store_sim.clone())?);
    write(store, 0, &[quad("b", "knows", ex("c"))])?;
    let _take_log: Vec<OpRecord> = store_sim.take_log();
    let ts = write_at(store, 0, &[quad("new", "knows", ex("b"))])?;
    w.log = store_sim.take_log();
    w.ts = vec![ts];
    Ok(())
}

#[then("the write reads no forward, subject-predicate or statement key of `<new>`")]
fn fresh_unread(w: &mut Steps) -> TestResult {
    let new = id(w.store.as_ref().context("the store")?, 0, &ex("new"))?;
    let names_new = |key: &[u8]| match QuadKey::parse(key) {
        Ok((QuadKey::Forward { subject, .. }, _))
        | Ok((QuadKey::SubjectPredicates { subject, .. }, _)) => subject == new,
        Ok((QuadKey::Statement { .. }, _)) => true,
        _ => false,
    };
    let mut lookups = 0;
    for record in &w.log {
        let keys: Vec<&[u8]> = if let OpRecord::Read { key, .. } = record {
            vec![key]
        } else if let OpRecord::ReadMany { keys, .. } = record {
            keys.iter().map(Vec::as_slice).collect()
        } else if let OpRecord::Scanned { scan, .. } = record {
            vec![scan.start()]
        } else {
            lookups += usize::from(matches!(record, OpRecord::ReadVersionless { .. }));
            vec![]
        };
        assert!(!keys.iter().any(|k| names_new(k)), "read {record:?}");
    }
    assert!(
        lookups > 0,
        "the write looked its terms up in the dictionary, which is no list"
    );
    Ok(())
}

#[when("a store written under contract version 2 is opened by a build implementing version 1")]
fn version_2(w: &mut Steps) -> TestResult {
    let store_sim = sim();
    let mut batch = WriteBatch::new();
    batch.put_versionless(
        CONTRACT_KEY.to_vec(),
        ContractVersion::new(2, 0).to_bytes().to_vec(),
    );
    store_sim.apply(batch, PersistMode::Sync)?;
    let before = store_sim.contents();
    let cache = SimListCache::new(16, Seed::new(1).stream(Stream::Cache));
    w.error = QuadStore::open(
        Lists::new(store_sim.clone(), cache, ListsConfig::default()),
        blocks(),
        RandomSource::new(1),
    )
    .err();
    assert_eq!(store_sim.contents(), before, "nothing is written");
    assert_eq!(CONTRACT_VERSION.major(), 1);
    Ok(())
}

#[then("opening fails with an error naming contract versions 2 and 1, and nothing is written")]
fn version_refused(w: &mut Steps) -> TestResult {
    let text = w.error.take().context("refused")?.to_string();
    assert!(
        text.contains("version 2.0") && text.contains("version 1.0"),
        "{text}"
    );
    Ok(())
}

#[when("a seeded workload creates terms and writes quads naming them under every fault class")]
fn dictionary_workload(w: &mut Steps) -> TestResult {
    for seed in 300..308 {
        let (_, checked) = crate::utils::dictionary_workload::DictionaryWorkload::run(
            Seed::new(seed),
            crate::utils::dictionary_workload::DictionaryWorkload::faults(),
            150,
        )
        .map_err(|v| v.to_string())
        .context("the workload breaks no invariant")?;
        w.ts.push(checked as u64);
    }
    Ok(())
}

#[then(
    "every stored UID and literal hash has its dictionary entry after every step and every recovery"
)]
fn dictionary_complete(w: &mut Steps) {
    assert!(w.ts.iter().sum::<u64>() > 200, "{:?} lists checked", w.ts);
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
