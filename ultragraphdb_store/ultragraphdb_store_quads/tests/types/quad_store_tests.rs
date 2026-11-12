/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{BlankNode, GraphName, Literal, Quad, Term};
use ultragraphdb_core_codec::Posting;
use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_keys::{Graph, QuadKey};
use ultragraphdb_core_mapping::RandomSource;
use ultragraphdb_core_terms::{EncodedTerm, Hash128};
use ultragraphdb_store_posting::SET;
use ultragraphdb_store_quads::{
    QuadErrorEnum, QuadStore, StatementRecord, WriteTerms, id_key, literal_key, term_key,
};
use ultragraphdb_store_sim::SimStore;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

use crate::utils::helpers::{
    TestQuads, blocks, commit, ex, forward, id, insert_write, lists_over, named, quad, quads,
    quads_over, sim, write, write_at,
};

fn open(store: SimStore) -> TestResult<TestQuads> {
    Ok(QuadStore::open(
        lists_over(store),
        blocks(),
        RandomSource::new(1),
    )?)
}

#[test]
fn test_a_new_store_records_the_contract_and_a_store_with_data_but_none_is_refused() -> TestResult {
    let store = sim();
    let _recorded: TestQuads = open(store.clone())?;
    let _reopened: TestQuads = open(store).context("its own version opens again")?;
    let old = sim();
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(b"\x00stage-2 key".to_vec(), 1, 4, vec![]));
    old.apply(batch, PersistMode::Sync)?;
    let error = open(old).err().context("refused")?;
    assert!(error.to_string().contains("version 0.0"), "{error}");
    Ok(())
}

#[test]
fn test_a_blank_node_is_skolemized_once_per_write_and_presented_as_a_blank_node() -> TestResult {
    let store = quads()?;
    let x = BlankNode::new_unchecked("x");
    let ts = write_at(
        &store,
        0,
        &[
            Quad::new(x.clone(), ex("knows"), ex("b"), GraphName::DefaultGraph),
            Quad::new(ex("c"), ex("knows"), x.clone(), GraphName::DefaultGraph),
        ],
    )?;
    write(
        &store,
        0,
        &[Quad::new(x, ex("knows"), ex("d"), GraphName::DefaultGraph)],
    )?;
    let reverse = |o: &str| -> TestResult<Vec<u8>> {
        Ok(QuadKey::Reverse {
            tenant: 0,
            graph: Graph::Default,
            predicate: id(&store, 0, &ex("knows"))?,
            object: EncodedTerm::id(id(&store, 0, &ex(o))?),
        }
        .to_bytes())
    };
    let subjects_of_b = store.list_terms(0, &reverse("b")?, u64::MAX)?;
    let targets_of_c = store.list_terms(
        0,
        &forward(&store, 0, Graph::Default, &ex("c"), &ex("knows"))?,
        ts,
    )?;
    let Some(Term::BlankNode(blank)) = subjects_of_b.first() else {
        return Err(ultragraphdb_tool_bdd::TestError::new(format!(
            "a blank node: {subjects_of_b:?}"
        )));
    };
    assert_eq!(
        subjects_of_b, targets_of_c,
        "one blank node for every statement of the write naming _:x"
    );
    let subjects_of_d = store.list_terms(0, &reverse("d")?, u64::MAX)?;
    assert_ne!(
        subjects_of_b, subjects_of_d,
        "another write's _:x is another node"
    );
    // The label is `b`, the skolem IRI's bucket, `-` and its random digits: no part of the ID but its bucket.
    let (label_bucket, random) = blank
        .as_str()
        .strip_prefix('b')
        .and_then(|l| l.split_once('-'))
        .context("a label")?;
    assert_eq!((label_bucket.len(), random.len()), (6, 26), "{blank}");
    let skolem = store
        .objects(
            &forward(&store, 0, Graph::Default, &ex("c"), &ex("knows"))?,
            ts,
        )?
        .first()
        .and_then(|(o, _)| o.as_id())
        .context("an ID object")?;
    let (text, is_blank) = store.iri_of(0, skolem)?;
    assert!(is_blank);
    assert!(
        text.ends_with(&format!("{label_bucket}-{random}")),
        "{text}"
    );
    assert_eq!(
        u32::from_str_radix(label_bucket, 16)?,
        skolem.bucket().get()
    );
    Ok(())
}

#[test]
fn test_literals_of_every_kind_read_back_exactly() -> TestResult {
    let store = quads()?;
    let objects: Vec<Term> = vec![
        Literal::new_typed_literal("01", oxrdf::vocab::xsd::INTEGER).into(),
        Literal::new_typed_literal("twelve", oxrdf::vocab::xsd::INTEGER).into(),
        Literal::new_language_tagged_literal("a long label in English", "en")?.into(),
        Literal::new_typed_literal("x", ex("custom")).into(),
        Literal::new_simple_literal("a string longer than fifteen bytes").into(),
    ];
    let quads: Vec<Quad> = objects.iter().map(|o| quad("a", "v", o.clone())).collect();
    let ts = write_at(&store, 0, &quads)?;
    let mut read = store.list_terms(
        0,
        &forward(&store, 0, Graph::Default, &ex("a"), &ex("v"))?,
        ts,
    )?;
    let mut expected = objects;
    read.sort_by_key(Term::to_string);
    expected.sort_by_key(Term::to_string);
    assert_eq!(read, expected);
    Ok(())
}

#[test]
fn test_two_writes_of_one_batch_creating_one_iri_both_commit_under_one_id() -> TestResult {
    let store = quads()?;
    let first = insert_write(&store, 0, &[quad("n", "p", ex("q"))])?;
    let second = insert_write(&store, 0, &[quad("n", "r", ex("s"))])?;
    let versions = commit(&store, vec![first, second])?;
    assert_eq!(versions.len(), 2);
    assert!(versions[0] < versions[1], "{versions:?}");
    let n = id(&store, 0, &ex("n"))?;
    let objects = |p: &str| -> TestResult<Vec<Term>> {
        let key = forward(&store, 0, Graph::Default, &ex("n"), &ex(p))?;
        Ok(store.list_terms(0, &key, u64::MAX)?)
    };
    assert_eq!(objects("p")?, vec![Term::from(ex("q"))]);
    assert_eq!(objects("r")?, vec![Term::from(ex("s"))]);
    assert_eq!(store.iri_of(0, n)?, (ex("n").as_str().to_string(), false));
    Ok(())
}

#[test]
fn test_a_commit_writes_the_entries_of_new_terms_and_settles_them() -> TestResult {
    let store = quads()?;
    let mut creator = WriteTerms::new(0);
    let created = creator.iri(&store, ex("new").as_str())?;
    assert_eq!(
        store.id_of(0, ex("new").as_str())?,
        None,
        "a pending ID is not committed"
    );
    let mut batch = store.lists().batch();
    batch.push(store.lists().request(store.lists().applied_ts()));
    let _versions: Vec<u64> = store.commit(batch, &[creator])?;
    assert_eq!(store.id_of(0, ex("new").as_str())?, Some(created));
    assert_eq!(
        store.store().get_versionless(&id_key(0, created))?,
        Some(ex("new").as_str().as_bytes().to_vec())
    );
    let mut later = WriteTerms::new(0);
    assert_eq!(later.iri(&store, ex("new").as_str())?, created);
    assert!(!later.is_fresh(created), "held");
    assert_eq!(
        later.written().count(),
        0,
        "settled: the dictionary holds it"
    );
    Ok(())
}

#[test]
fn test_a_released_write_leaves_no_dictionary_entry_and_burns_its_ids() -> TestResult {
    let store = quads()?;
    let before = store.store().contents();
    let mut refused = WriteTerms::new(0);
    let gone = refused.iri(&store, ex("gone").as_str())?;
    let blank = refused.blank(&store, "x")?;
    store.release(&refused);
    assert_eq!(store.store().contents(), before, "nothing is written");
    assert_eq!(store.id_of(0, ex("gone").as_str())?, None);
    assert!(store.iri_of(0, blank).is_err(), "no entry names the blank");
    let mut later = WriteTerms::new(0);
    let again = later.iri(&store, ex("gone").as_str())?;
    assert_ne!(again, gone, "the released ID is burned");
    assert!(later.is_fresh(again), "the later write creates it");
    assert_eq!(
        later.written().collect::<Vec<_>>(),
        vec![(ex("gone").as_str(), again)]
    );
    Ok(())
}

/// A write continuing another on a clone of its terms releases what it took beyond them alone: an ID only it created is burned,
/// and the base's own stays pending for it.
#[test]
fn test_a_release_beyond_a_base_lets_go_of_what_the_continuation_took_alone() -> TestResult {
    let store = quads()?;
    let mut base = WriteTerms::new(0);
    let kept = base.iri(&store, ex("kept").as_str())?;
    let mut continued = base.clone();
    let beyond = continued.iri(&store, ex("beyond").as_str())?;
    store.release_beyond(&continued, &base);
    let mut later = WriteTerms::new(0);
    assert_ne!(
        later.iri(&store, ex("beyond").as_str())?,
        beyond,
        "the continuation's own ID is burned"
    );
    assert_eq!(
        later.iri(&store, ex("kept").as_str())?,
        kept,
        "the base still holds its own"
    );
    Ok(())
}

#[test]
fn test_a_write_that_took_a_refused_creators_pending_id_carries_and_commits_its_entries()
-> TestResult {
    let store = quads()?;
    let mut creator = WriteTerms::new(0);
    let created = creator.iri(&store, ex("new").as_str())?;
    let mut taker = WriteTerms::new(0);
    assert_eq!(taker.iri(&store, ex("new").as_str())?, created);
    store.release(&creator);
    assert_eq!(store.id_of(0, ex("new").as_str())?, None);
    let mut third = WriteTerms::new(0);
    assert_eq!(
        third.iri(&store, ex("new").as_str())?,
        created,
        "still pending: the taker holds it"
    );
    store.release(&third);
    assert!(
        taker.entries().contains(&(
            term_key(0, ex("new").as_str()),
            created.to_be_bytes().to_vec()
        )),
        "the taker carries the entries"
    );
    let mut batch = store.lists().batch();
    batch.push(store.lists().request(store.lists().applied_ts()));
    let _versions: Vec<u64> = store.commit(batch, &[taker])?;
    assert_eq!(store.id_of(0, ex("new").as_str())?, Some(created));
    assert_eq!(
        store.iri_of(0, created)?,
        (ex("new").as_str().to_string(), false)
    );
    Ok(())
}

#[test]
fn test_a_pending_id_is_burned_once_every_write_holding_it_is_released() -> TestResult {
    let store = quads()?;
    let (mut creator, mut taker) = (WriteTerms::new(0), WriteTerms::new(0));
    let created = creator.iri(&store, ex("new").as_str())?;
    let _taken: SystemId = taker.iri(&store, ex("new").as_str())?;
    store.release(&taker);
    store.release(&creator);
    assert_ne!(
        WriteTerms::new(0).iri(&store, ex("new").as_str())?,
        created,
        "no write holds it: the next writer creates another"
    );
    Ok(())
}

#[test]
fn test_a_write_dropped_without_release_leaves_its_id_pending_for_the_next_writer() -> TestResult {
    let store = quads()?;
    let dropped = insert_write(&store, 0, &[quad("new", "p", ex("q"))])?;
    let (_, terms) = dropped.finish()?;
    let created = terms
        .written()
        .next()
        .map(|(_, id)| id)
        .context("created")?;
    drop(terms);
    let later = write_at(&store, 0, &[quad("new", "p", ex("q"))])?;
    assert!(later > 0);
    assert_eq!(id(&store, 0, &ex("new"))?, created, "one ID for the IRI");
    Ok(())
}

#[test]
fn test_a_missing_entry_is_corruption_naming_its_key() -> TestResult {
    let store = quads()?;
    let lost = SystemId::new(TypePrefix::RESOURCE, Bucket::new(5)?, Sequence::new(77)?);
    let error = store.iri_of(0, lost).err().context("refused")?;
    assert!(
        matches!(&error.0, QuadErrorEnum::Corrupted { key, .. } if *key == id_key(0, lost)),
        "{error:?}"
    );
    let hex: String = id_key(0, lost).iter().map(|b| format!("{b:02x}")).collect();
    assert!(error.to_string().contains(&hex), "{error}");
    let hash = Hash128::of(b"never written");
    let missing = store.string_of(0, hash).err().context("refused")?;
    assert!(
        matches!(&missing.0, QuadErrorEnum::Corrupted { key, .. } if *key == literal_key(0, &hash)),
        "{missing:?}"
    );
    Ok(())
}

#[test]
fn test_an_unreadable_entry_is_corruption() -> TestResult {
    let store = quads()?;
    let id = SystemId::new(TypePrefix::RESOURCE, Bucket::new(5)?, Sequence::new(78)?);
    let hash = Hash128::of(b"bad");
    let mut batch = WriteBatch::new();
    batch.put_versionless(term_key(0, "http://ex.org/short"), vec![0, 1, 2]);
    batch.put_versionless(term_key(0, "http://ex.org/invalid"), vec![0; 16]);
    batch.put_versionless(id_key(0, id), vec![0xff, 0xfe]);
    batch.put_versionless(literal_key(0, &hash), vec![0xc3]);
    store.store().apply(batch, PersistMode::Sync)?;
    for iri in ["http://ex.org/short", "http://ex.org/invalid"] {
        let error = store.id_of(0, iri).err().context("refused")?;
        assert!(
            matches!(&error.0, QuadErrorEnum::Corrupted { key, .. } if *key == term_key(0, iri)),
            "{error:?}"
        );
        let resolved = WriteTerms::new(0)
            .iri(&store, iri)
            .err()
            .context("refused")?;
        assert!(
            matches!(resolved.0, QuadErrorEnum::Corrupted { .. }),
            "{resolved:?}"
        );
    }
    assert!(matches!(
        store.iri_of(0, id).err().context("refused")?.0,
        QuadErrorEnum::Corrupted { .. }
    ));
    assert!(matches!(
        store.string_of(0, hash).err().context("refused")?.0,
        QuadErrorEnum::Corrupted { .. }
    ));
    Ok(())
}

#[test]
fn test_a_posting_naming_no_valid_id_is_corruption_naming_its_list() -> TestResult {
    let store = quads()?;
    write(&store, 0, &[quad("a", "p", ex("b"))])?;
    let key = forward(&store, 0, Graph::Default, &ex("a"), &ex("p"))?;
    let mut request = store.lists().request(store.lists().applied_ts());
    // 1 as a 128-bit key: type 0, sequence 0, which no system ID is.
    request
        .get_from_delta(&key)
        .add_posting(Posting::default().with_uid(1).with_op(SET), false);
    let mut batch = store.lists().batch();
    batch.push(request);
    let _versions: Vec<u64> = store.lists().commit(batch)?;
    let error = store
        .list_terms(0, &key, u64::MAX)
        .err()
        .context("refused")?;
    assert!(
        matches!(&error.0, QuadErrorEnum::Corrupted { key: k, .. } if *k == key),
        "{error:?}"
    );
    let mut reading = insert_write(&store, 0, &[])?;
    let (a, p) = (id(&store, 0, &ex("a"))?, id(&store, 0, &ex("p"))?);
    let read = reading
        .objects(Graph::Default, a, p)
        .err()
        .context("refused")?;
    assert!(
        matches!(&read.0, QuadErrorEnum::Corrupted { key: k, .. } if *k == key),
        "{read:?}"
    );
    let objects = store.objects(&key, u64::MAX).err().context("refused")?;
    assert!(
        matches!(&objects.0, QuadErrorEnum::Corrupted { key: k, .. } if *k == key),
        "{objects:?}"
    );
    Ok(())
}

#[test]
fn test_objects_carry_their_statement_ids_and_records_read_back() -> TestResult {
    let store = quads()?;
    let mut write = insert_write(&store, 0, &[])?;
    let (a, p, b) = {
        let (terms, q) = write.terms();
        (
            terms.iri(q, ex("a").as_str())?,
            terms.iri(q, ex("p").as_str())?,
            terms.iri(q, ex("b").as_str())?,
        )
    };
    let statement = write.next_statement_id(a)?;
    let object = EncodedTerm::id(b);
    write.insert(Graph::Default, a, p, &object, &[statement], "iri");
    let record = StatementRecord::new(a, p, object.clone(), vec![(p, EncodedTerm::id(a))]);
    write.put_statement(Graph::Default, statement, &record);
    let ts = commit(&store, vec![write])?
        .first()
        .copied()
        .context("committed")?;
    let key = QuadKey::Forward {
        tenant: 0,
        graph: Graph::Default,
        predicate: p,
        subject: a,
    }
    .to_bytes();
    assert_eq!(store.objects(&key, ts)?, vec![(object, vec![statement])]);
    assert_eq!(
        store.statement(0, Graph::Default, statement, ts)?,
        Some(record)
    );
    assert_eq!(
        store.statement(0, Graph::Default, statement, ts - 1)?,
        None,
        "not before its commit"
    );
    assert_eq!(
        store.statement(0, Graph::Named(a), statement, ts)?,
        None,
        "another graph"
    );
    assert_eq!(named(&store, 0, &ex("a"))?, Graph::Named(a));
    Ok(())
}

#[test]
fn test_a_reifier_reads_back_as_its_iri() -> TestResult {
    let store = quads()?;
    let mut write = insert_write(&store, 0, &[])?;
    let a = {
        let (terms, q) = write.terms();
        terms.iri(q, ex("a").as_str())?
    };
    let statement = write.next_statement_id(a)?;
    let ts = commit(&store, vec![write])?
        .first()
        .copied()
        .context("committed")?;
    let (reifier, is_blank) = store.iri_of(0, statement)?;
    assert!(!is_blank);
    assert_eq!(
        store.term_of(0, &EncodedTerm::id(statement))?,
        Term::from(oxrdf::NamedNode::new(reifier)?)
    );
    assert!(ts > 0);
    Ok(())
}

#[test]
fn test_id_of_reads_the_committed_dictionary_alone() -> TestResult {
    let store = quads_over(sim())?;
    assert_eq!(store.id_of(0, ex("a").as_str())?, None);
    write(&store, 0, &[quad("a", "p", ex("b"))])?;
    let a = store.id_of(0, ex("a").as_str())?.context("committed")?;
    assert_eq!(
        a.bucket().get(),
        0xC7_F1AB,
        "core_ids' golden bucket of http://ex.org/a in tenant 0"
    );
    assert_eq!(store.id_of(1, ex("a").as_str())?, None, "another tenant");
    Ok(())
}
