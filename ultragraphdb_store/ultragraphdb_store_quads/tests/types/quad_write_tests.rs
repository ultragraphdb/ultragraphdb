/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{Literal, Term};
use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_keys::{Graph, QuadKey};
use ultragraphdb_core_mapping::RandomSource;
use ultragraphdb_core_terms::{EncodedTerm, TermError};
use ultragraphdb_store_quads::{QuadStore, QuadWrite};

use crate::utils::helpers::{
    TestQuads, TestWrite, blocks, commit, ex, forward, id, lists_over, quad, quads, sim, write,
};
use ultragraphdb_tool_bdd::TestResult;

const D: Graph = Graph::Default;

/// A statement ID of sequence `n` in bucket 9, as a posting carries it.
fn statement(n: u64) -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::STATEMENT,
        Bucket::new(9)?,
        Sequence::new(n)?,
    ))
}

fn literal(text: &str) -> TestResult<EncodedTerm> {
    Ok(
        EncodedTerm::literal(Literal::new_simple_literal(text).as_ref(), |_| {
            Err(TermError::malformed(
                Vec::new(),
                "a simple literal names no datatype",
            ))
        })?
        .into_term(),
    )
}

fn delete(
    store: &TestQuads,
    s: &str,
    p: &str,
    object: &EncodedTerm,
) -> TestResult<Option<Vec<SystemId>>> {
    let mut w = QuadWrite::new(store, 0, store.lists().request(store.lists().applied_ts()));
    let (s, p) = (id(store, 0, &ex(s))?, id(store, 0, &ex(p))?);
    let deleted = w.delete(D, s, p, object, "x")?;
    let _committed_at: Vec<u64> = commit(store, vec![w])?;
    Ok(deleted)
}

#[test]
fn test_a_delete_removes_what_is_held_and_keeps_predicates_still_in_use() -> TestResult {
    let store = quads()?;
    write(
        &store,
        0,
        &[
            quad("a", "nick", Literal::new_simple_literal("x")),
            quad("a", "nick", Literal::new_simple_literal("y")),
        ],
    )?;
    write(
        &store,
        0,
        &[quad("c", "nick", Literal::new_simple_literal("x"))],
    )?;
    let (a, nick) = (id(&store, 0, &ex("a"))?, id(&store, 0, &ex("nick"))?);
    let preds = |subject: SystemId| {
        store.list_terms(
            0,
            &QuadKey::SubjectPredicates {
                tenant: 0,
                graph: D,
                subject,
            }
            .to_bytes(),
            u64::MAX,
        )
    };
    let object_preds = |object: &EncodedTerm| {
        store.list_terms(
            0,
            &QuadKey::ObjectPredicates {
                tenant: 0,
                graph: D,
                object: object.clone(),
            }
            .to_bytes(),
            u64::MAX,
        )
    };
    assert_eq!(
        delete(&store, "a", "nick", &literal("z")?)?,
        None,
        "an object not held changes nothing"
    );
    assert!(delete(&store, "a", "nick", &literal("x")?)?.is_some());
    assert_eq!(preds(a)?, vec![Term::from(ex("nick"))], "a still holds y");
    assert_eq!(
        object_preds(&literal("x")?)?,
        vec![Term::from(ex("nick"))],
        "c still points at x"
    );
    let _deleted: Option<Vec<SystemId>> = delete(&store, "a", "nick", &literal("y")?)?;
    assert!(preds(a)?.is_empty());
    assert!(object_preds(&literal("y")?)?.is_empty());
    let stats = store.statistics(0, D, nick, u64::MAX)?;
    assert_eq!(stats.statements(), 1);
    Ok(())
}

#[test]
fn test_colliding_fingerprints_keep_both_literals_and_deleting_one_keeps_the_other() -> TestResult {
    fn constant(_: &[u8]) -> u64 {
        42
    }
    let store: TestQuads = QuadStore::open(lists_over(sim()), blocks(), RandomSource::new(1))?
        .with_fingerprint(constant);
    write(
        &store,
        0,
        &[
            quad("a", "name", Literal::new_simple_literal("A")),
            quad("a", "name", Literal::new_simple_literal("B")),
        ],
    )?;
    let key = forward(&store, 0, D, &ex("a"), &ex("name"))?;
    let _rollup_key: bool = store.lists().rollup_key(&key)?;
    let mut held = store.list_terms(0, &key, u64::MAX)?;
    held.sort_by_key(Term::to_string);
    assert_eq!(
        held,
        vec![
            Literal::new_simple_literal("A").into(),
            Literal::new_simple_literal("B").into()
        ]
    );
    let _deleted: Option<Vec<SystemId>> = delete(&store, "a", "name", &literal("A")?)?;
    assert_eq!(
        store.list_terms(0, &key, u64::MAX)?,
        vec![Term::from(Literal::new_simple_literal("B"))]
    );
    Ok(())
}

/// Under colliding fingerprints a literal is held only by its own value posting: the list holding `"A"` holds `"A"` and not
/// `"B"`, whose posting would share its uid.
#[test]
fn test_a_literal_is_held_by_its_own_value_under_a_colliding_fingerprint() -> TestResult {
    fn constant(_: &[u8]) -> u64 {
        42
    }
    let store: TestQuads = QuadStore::open(lists_over(sim()), blocks(), RandomSource::new(1))?
        .with_fingerprint(constant);
    write(
        &store,
        0,
        &[quad("a", "name", Literal::new_simple_literal("A"))],
    )?;
    let (s, p) = (id(&store, 0, &ex("a"))?, id(&store, 0, &ex("name"))?);
    let mut w = QuadWrite::new(&store, 0, store.lists().request(store.lists().applied_ts()));
    assert_eq!(w.holds(D, s, p, &literal("A")?)?, Some(Vec::new()));
    assert_eq!(w.holds(D, s, p, &literal("B")?)?, None);
    Ok(())
}

/// A write with the subject's and predicate's UIDs and the object's encoding it resolved.
type Resolved<'a> = (TestWrite<'a>, SystemId, SystemId, EncodedTerm);

/// A write over a request at the newest timestamp, the terms of `<s>`, `<p>` and `<o>` resolved: the subject's and
/// predicate's UIDs and the object's encoding.
fn resolved<'a>(store: &'a TestQuads, s: &str, p: &str, o: &str) -> TestResult<Resolved<'a>> {
    let mut w = QuadWrite::new(store, 0, store.lists().request(store.lists().applied_ts()));
    let (terms, quads) = w.terms();
    let subject = terms.iri(quads, ex(s).as_str())?;
    let predicate = terms.iri(quads, ex(p).as_str())?;
    let object = EncodedTerm::id(terms.iri(quads, ex(o).as_str())?);
    Ok((w, subject, predicate, object))
}

/// A statement held is found with its statement IDs, and the write records the statement it tested, by the subject's
/// forward key and the object's encoding, and no list read whole.
#[test]
fn test_a_held_statement_is_found_and_recorded_as_a_statement() -> TestResult {
    let store = quads()?;
    write(
        &store,
        0,
        &[quad("a", "knows", ex("b")), quad("a", "knows", ex("c"))],
    )?;
    let (mut w, s, p, o) = resolved(&store, "a", "knows", "b")?;
    assert_eq!(w.holds(D, s, p, &o)?, Some(Vec::new()));
    let key = forward(&store, 0, D, &ex("a"), &ex("knows"))?;
    let request = w.request();
    assert_eq!(
        request.tested().collect::<Vec<_>>(),
        vec![(key.as_slice(), o.as_bytes())]
    );
    assert_eq!(request.read_keys().count(), 0);
    Ok(())
}

/// A statement the subject does not hold, beside others it does, is not found, and the test is recorded all the same:
/// it is what a commit adding the object would change.
#[test]
fn test_a_statement_not_held_is_recorded_too() -> TestResult {
    let store = quads()?;
    write(
        &store,
        0,
        &[quad("a", "knows", ex("c")), quad("x", "y", ex("b"))],
    )?;
    let (mut w, s, p, o) = resolved(&store, "a", "knows", "b")?;
    assert_eq!(w.holds(D, s, p, &o)?, None);
    let key = forward(&store, 0, D, &ex("a"), &ex("knows"))?;
    assert_eq!(
        w.request().tested().collect::<Vec<_>>(),
        vec![(key.as_slice(), o.as_bytes())]
    );
    assert_eq!(w.request().read_keys().count(), 0);
    Ok(())
}

/// The statement IDs of a held statement are those its posting carries.
/// The objects of the reverse list of `<p>` for `object`, each with its statement IDs.
fn reverse_postings(
    store: &TestQuads,
    p: &str,
    object: &EncodedTerm,
) -> TestResult<Vec<(EncodedTerm, Vec<SystemId>)>> {
    let key = QuadKey::Reverse {
        tenant: 0,
        graph: D,
        predicate: id(store, 0, &ex(p))?,
        object: object.clone(),
    }
    .to_bytes();
    Ok(store.objects(&key, store.lists().applied_ts())?)
}

/// An insert and a rewrite of a statement's IDs each write its forward and its reverse posting, a vector its forward alone.
#[test]
fn test_set_statement_ids_writes_both_postings() -> TestResult {
    let store = quads()?;
    write(&store, 0, &[quad("a", "knows", ex("b"))])?;
    let (a, knows, b) = (
        id(&store, 0, &ex("a"))?,
        id(&store, 0, &ex("knows"))?,
        EncodedTerm::id(id(&store, 0, &ex("b"))?),
    );
    let subject = EncodedTerm::id(a);
    assert_eq!(
        reverse_postings(&store, "knows", &b)?,
        vec![(subject.clone(), Vec::new())]
    );
    let ids = [statement(1)?, statement(2)?];
    let mut w = QuadWrite::new(&store, 0, store.lists().request(store.lists().applied_ts()));
    w.set_statement_ids(D, a, knows, &b, &ids);
    let _set_at: Vec<u64> = commit(&store, vec![w])?;
    assert_eq!(
        reverse_postings(&store, "knows", &b)?,
        vec![(subject.clone(), ids.to_vec())]
    );
    let held = store.objects(
        &forward(&store, 0, D, &ex("a"), &ex("knows"))?,
        store.lists().applied_ts(),
    )?;
    assert_eq!(held, vec![(b.clone(), ids.to_vec())]);
    let c = EncodedTerm::id(id(&store, 0, &ex("a"))?);
    let mut inserting =
        QuadWrite::new(&store, 0, store.lists().request(store.lists().applied_ts()));
    inserting.insert(D, knows, knows, &c, &[statement(3)?], "iri");
    let _inserted_at: Vec<u64> = commit(&store, vec![inserting])?;
    assert_eq!(
        reverse_postings(&store, "knows", &c)?,
        vec![(EncodedTerm::id(knows), vec![statement(3)?])]
    );
    Ok(())
}

/// A vector names no reverse list, so its statement IDs reach its forward posting alone.
#[test]
fn test_set_statement_ids_of_a_vector_writes_its_forward_posting_alone() -> TestResult {
    let store = quads()?;
    write(&store, 0, &[quad("a", "knows", ex("b"))])?;
    let (a, knows) = (id(&store, 0, &ex("a"))?, id(&store, 0, &ex("knows"))?);
    let vector = EncodedTerm::from_bytes(&[0x40, 1, 0, 0, 0, 128, 63])?;
    assert!(vector.is_vector());
    let mut w = QuadWrite::new(&store, 0, store.lists().request(store.lists().applied_ts()));
    w.set_statement_ids(D, a, knows, &vector, &[statement(1)?]);
    let (request, _terms) = w.finish()?;
    let written: Vec<Vec<u8>> = request.written_keys().map(<[u8]>::to_vec).collect();
    assert_eq!(
        written,
        vec![forward(&store, 0, D, &ex("a"), &ex("knows"))?]
    );
    Ok(())
}

#[test]
fn test_a_held_statement_carries_its_statement_ids() -> TestResult {
    let store = quads()?;
    let (mut w, s, p, o) = resolved(&store, "a", "knows", "b")?;
    w.insert(D, s, p, &o, &[statement(7)?, statement(9)?], "iri");
    let _committed_at: Vec<u64> = commit(&store, vec![w])?;
    let (mut reader, subject, predicate, object) = resolved(&store, "a", "knows", "b")?;
    assert_eq!(
        reader.holds(D, subject, predicate, &object)?,
        Some(vec![statement(7)?, statement(9)?])
    );
    Ok(())
}

/// A subject the write created holds nothing, so its test reads no list and records nothing.
#[test]
fn test_a_subject_the_write_created_records_no_test() -> TestResult {
    let store = quads()?;
    write(&store, 0, &[quad("x", "knows", ex("b"))])?;
    let (mut w, s, p, o) = resolved(&store, "new", "knows", "b")?;
    assert!(w.is_fresh(s));
    assert_eq!(w.holds(D, s, p, &o)?, None);
    assert_eq!(w.request().tested().count(), 0);
    assert_eq!(w.request().read_keys().count(), 0);
    w.insert(D, s, p, &o, &[], "iri");
    assert_eq!(
        w.holds(D, s, p, &o)?,
        Some(Vec::new()),
        "its own insert is seen"
    );
    assert_eq!(w.request().tested().count(), 0);
    Ok(())
}

/// The write sees its own inserts and deletes: a statement it inserted is held, one it deleted is not.
#[test]
fn test_a_test_sees_the_writes_own_insert_and_delete() -> TestResult {
    let store = quads()?;
    write(
        &store,
        0,
        &[quad("a", "knows", ex("b")), quad("a", "knows", ex("c"))],
    )?;
    let (mut w, s, p, b) = resolved(&store, "a", "knows", "b")?;
    let d = {
        let (terms, quads) = w.terms();
        EncodedTerm::id(terms.iri(quads, ex("d").as_str())?)
    };
    w.insert(D, s, p, &d, &[], "iri");
    assert_eq!(w.holds(D, s, p, &d)?, Some(Vec::new()));
    assert!(w.delete(D, s, p, &b, "iri")?.is_some());
    assert_eq!(w.holds(D, s, p, &b)?, None);
    Ok(())
}

/// A delete keeps its read of the subject's list, which it needs whole to tell whether the predicate stays in the
/// subject's predicate list.
#[test]
fn test_a_delete_reads_the_subjects_list_whole() -> TestResult {
    let store = quads()?;
    write(&store, 0, &[quad("a", "knows", ex("b"))])?;
    let (mut w, s, p, o) = resolved(&store, "a", "knows", "b")?;
    let _deleted: Option<Vec<SystemId>> = w.delete(D, s, p, &o, "iri")?;
    let key = forward(&store, 0, D, &ex("a"), &ex("knows"))?;
    assert!(w.request().read_keys().any(|k| k == key.as_slice()));
    Ok(())
}

/// A write seeded with a finished write continues it: its terms resolve as they did, its deltas and its statistics are
/// the write's own, and the two commit as one write with one statistics delta per predicate (7.5).
#[test]
fn test_a_seeded_write_continues_the_write_it_was_seeded_with() -> TestResult {
    let store = quads()?;
    let iri = |w: &mut TestWrite<'_>, name: &str| {
        let (terms, quads) = w.terms();
        terms.iri(quads, ex(name).as_str())
    };
    let read_ts = store.lists().applied_ts();
    let mut first = QuadWrite::new(&store, 0, store.lists().request(read_ts));
    let (s1, p) = (iri(&mut first, "s1")?, iri(&mut first, "p")?);
    first.insert(D, s1, p, &literal("a")?, &[], "x");
    let (request, terms) = first.finish()?;
    let writes = request.into_writes();

    let mut second = QuadWrite::new(&store, 0, store.lists().request(read_ts));
    second.seed(&writes, terms)?;
    assert_eq!(
        iri(&mut second, "s1")?,
        s1,
        "the seeded terms resolve as before"
    );
    let s2 = iri(&mut second, "s2")?;
    second.insert(D, s2, p, &literal("b")?, &[], "x");
    let _committed_at: Vec<u64> = commit(&store, vec![second])?;
    let stats = store.statistics(0, D, p, store.lists().applied_ts())?;
    assert_eq!(stats.statements(), 2, "both inserts counted, in one delta");
    assert_eq!(
        store
            .objects(
                &forward(&store, 0, D, &ex("s1"), &ex("p"))?,
                store.lists().applied_ts()
            )?
            .len(),
        1,
        "the seeded insert is written"
    );
    Ok(())
}

/// A new statement ID is of the `Statement` type in its subject's bucket; a record put at it reads back at the write's
/// commit, and a deleted one is gone.
#[test]
fn test_a_statement_id_names_a_record_until_the_record_is_deleted() -> TestResult {
    let store = quads()?;
    let (mut w, s, p, o) = resolved(&store, "a", "knows", "b")?;
    let id = w.next_statement_id(s)?;
    assert_eq!(
        (id.type_prefix(), id.bucket()),
        (TypePrefix::STATEMENT, s.bucket())
    );
    assert!(w.is_fresh(id));
    let record = ultragraphdb_store_quads::StatementRecord::new(s, p, o.clone(), Vec::new());
    w.insert(D, s, p, &o, &[id], "iri");
    w.put_statement(D, id, &record);
    assert_eq!(w.statement(D, id)?, None, "the write reads at its snapshot");
    let _committed_at: Vec<u64> = commit(&store, vec![w])?;
    let (mut reader, ..) = resolved(&store, "a", "knows", "b")?;
    assert_eq!(reader.statement(D, id)?, Some(record));
    assert_eq!(reader.holds(D, s, p, &o)?, Some(vec![id]));
    reader.delete_statement(D, id);
    reader.set_statement_ids(D, s, p, &o, &[]);
    let _deleted_at: Vec<u64> = commit(&store, vec![reader])?;
    let (mut after, ..) = resolved(&store, "a", "knows", "b")?;
    assert_eq!(after.statement(D, id)?, None);
    assert_eq!(
        after.holds(D, s, p, &o)?,
        Some(Vec::new()),
        "the posting no longer carries it"
    );
    Ok(())
}

/// The subjects of an object are its reverse list, read whole; a vector has none, and an object the write created
/// reads no list.
#[test]
fn test_subjects_read_the_reverse_list() -> TestResult {
    let store = quads()?;
    write(
        &store,
        0,
        &[quad("a", "knows", ex("b")), quad("c", "knows", ex("b"))],
    )?;
    let (mut w, _, p, o) = resolved(&store, "a", "knows", "b")?;
    let mut subjects = w.subjects(D, p, &o)?;
    subjects.sort();
    let mut expected = vec![id(&store, 0, &ex("a"))?, id(&store, 0, &ex("c"))?];
    expected.sort();
    assert_eq!(subjects, expected);
    assert_eq!(w.request().read_keys().count(), 1);
    let (mut fresh, _, knows, new) = resolved(&store, "x", "knows", "new")?;
    assert!(fresh.subjects(D, knows, &new)?.is_empty());
    assert_eq!(fresh.request().read_keys().count(), 0);
    let vector = EncodedTerm::literal(
        Literal::new_typed_literal(
            "[1, 2]",
            oxrdf::NamedNode::new(ultragraphdb_core_terms::FLOAT32_VECTOR)?,
        )
        .as_ref(),
        |_| {
            Err(TermError::malformed(
                Vec::new(),
                "a vector names no datatype ID",
            ))
        },
    )?
    .into_term();
    assert!(fresh.subjects(D, knows, &vector)?.is_empty());
    assert_eq!(fresh.request().read_keys().count(), 0);
    assert_eq!(QuadWrite::tenant(&fresh), 0);
    Ok(())
}

#[test]
fn test_predicate_lists_name_what_the_write_sees_and_are_recorded_whole() -> TestResult {
    let store = quads()?;
    write(&store, 0, &[quad("a", "knows", ex("b"))])?;
    let (a, knows, b) = (
        id(&store, 0, &ex("a"))?,
        id(&store, 0, &ex("knows"))?,
        id(&store, 0, &ex("b"))?,
    );
    let read_ts = store.lists().applied_ts();
    let mut w = QuadWrite::new(&store, 0, store.lists().request(read_ts));
    let likes = {
        let (terms, q) = w.terms();
        terms.iri(q, ex("likes").as_str())?
    };
    assert!(w.names_predicate(D, a, knows)?);
    assert!(!w.names_predicate(D, a, likes)?);
    assert!(w.points_with(D, &EncodedTerm::id(b), knows)?);
    assert!(!w.points_with(D, &EncodedTerm::id(b), likes)?);
    // The write's own insert shows on top.
    w.insert(D, a, likes, &EncodedTerm::id(b), &[], "iri");
    assert!(w.names_predicate(D, a, likes)?);
    assert!(w.points_with(D, &EncodedTerm::id(b), likes)?);
    let sp = QuadKey::SubjectPredicates {
        tenant: 0,
        graph: D,
        subject: a,
    }
    .to_bytes();
    let op = QuadKey::ObjectPredicates {
        tenant: 0,
        graph: D,
        object: EncodedTerm::id(b),
    }
    .to_bytes();
    let read: Vec<&[u8]> = w.request().read_keys().collect();
    assert!(read.contains(&sp.as_slice()) && read.contains(&op.as_slice()));
    Ok(())
}

#[test]
fn test_ids_read_a_list_of_ids_whole_in_id_order() -> TestResult {
    let store = quads()?;
    write(
        &store,
        0,
        &[quad("a", "knows", ex("z")), quad("c", "knows", ex("z"))],
    )?;
    let (knows, z) = (id(&store, 0, &ex("knows"))?, id(&store, 0, &ex("z"))?);
    let reverse = QuadKey::Reverse {
        tenant: 0,
        graph: D,
        predicate: knows,
        object: EncodedTerm::id(z),
    }
    .to_bytes();
    let read_ts = store.lists().applied_ts();
    let mut w = QuadWrite::new(&store, 0, store.lists().request(read_ts));
    let mut expected = vec![id(&store, 0, &ex("a"))?, id(&store, 0, &ex("c"))?];
    expected.sort_by_key(|i| i.to_u128());
    assert_eq!(w.ids(&reverse)?, expected);
    assert!(w.request().read_keys().any(|k| k == reverse.as_slice()));
    assert!(w.ids(b"\x11 no such list")?.is_empty());
    Ok(())
}

/// The snapshot's objects leave out the write's own writes, a subject the write created holds none in it, and
/// discarding the lists leaves the write seeing its snapshot again, through the store it writes to.
#[test]
fn test_the_snapshots_objects_leave_out_the_writes_own() -> TestResult {
    let store = quads()?;
    write(&store, 0, &[quad("a", "knows", ex("b"))])?;
    let (mut w, s, p, b) = resolved(&store, "a", "knows", "b")?;
    assert!(std::ptr::eq(w.quads(), &store));
    let held = w.snapshot_objects(D, s, p)?;
    assert!(held.iter().any(|(object, _)| object == &b));
    w.insert(D, s, p, &literal("c")?, &[], "x");
    assert_eq!(
        w.snapshot_objects(D, s, p)?,
        held,
        "the insert is the write's own"
    );
    assert_eq!(w.objects(D, s, p)?.len(), held.len() + 1);
    w.discard_lists();
    assert_eq!(w.objects(D, s, p)?, held);
    let created = {
        let (terms, quads) = w.terms();
        terms.iri(quads, ex("new").as_str())?
    };
    assert!(w.is_fresh(created));
    assert!(w.snapshot_objects(D, created, p)?.is_empty());
    Ok(())
}
