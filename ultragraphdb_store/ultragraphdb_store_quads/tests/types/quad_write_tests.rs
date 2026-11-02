/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{Literal, Term};
use ultragraphdb_core_keys::QuadKey;
use ultragraphdb_core_terms::{EncodedTerm, TermError};
use ultragraphdb_store_posting::{Lists, ListsConfig};
use ultragraphdb_store_quads::{QuadStore, QuadWrite};
use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimListCache, SimStore, Stream};

use crate::utils::helpers::{TestQuads, commit, ex, forward, quad, quads, uid, write};
use ultragraphdb_tool_bdd::TestResult;

fn literal(text: &str) -> TestResult<EncodedTerm> {
    Ok(
        EncodedTerm::literal(Literal::new_simple_literal(text).as_ref(), |_| {
            Ok::<_, TermError>(0)
        })?
        .into_term(),
    )
}

fn delete(
    store: &TestQuads,
    s: &str,
    p: &str,
    object: &EncodedTerm,
) -> TestResult<Option<Vec<u64>>> {
    let mut w = QuadWrite::new(store, 0, store.lists().request(store.lists().applied_ts()));
    let (s, p) = (uid(store, 0, &ex(s))?, uid(store, 0, &ex(p))?);
    let deleted = w.delete(0, s, p, object, "x")?;
    let _committed_at: u64 = commit(store, vec![w])?.remove(0)?;
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
    let (a, nick) = (uid(&store, 0, &ex("a"))?, uid(&store, 0, &ex("nick"))?);
    let preds = |subject: u64| {
        store.list_terms(
            0,
            &QuadKey::SubjectPredicates {
                namespace: 0,
                graph: 0,
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
                namespace: 0,
                graph: 0,
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
    let _deleted: Option<Vec<u64>> = delete(&store, "a", "nick", &literal("y")?)?;
    assert!(preds(a)?.is_empty());
    assert!(object_preds(&literal("y")?)?.is_empty());
    let stats = store.statistics(0, 0, nick, u64::MAX)?;
    assert_eq!(stats.statements(), 1);
    Ok(())
}

#[test]
fn test_colliding_fingerprints_keep_both_literals_and_deleting_one_keeps_the_other() -> TestResult {
    fn constant(_: &[u8]) -> u64 {
        42
    }
    let lists = Lists::new(
        SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new()),
        SimListCache::new(16, Seed::new(1).stream(Stream::Cache)),
        ListsConfig::default(),
    );
    let store = QuadStore::open(lists)?.with_fingerprint(constant);
    write(
        &store,
        0,
        &[
            quad("a", "name", Literal::new_simple_literal("A")),
            quad("a", "name", Literal::new_simple_literal("B")),
        ],
    )?;
    let key = forward(&store, 0, 0, &ex("a"), &ex("name"))?;
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
    let _deleted: Option<Vec<u64>> = delete(&store, "a", "name", &literal("A")?)?;
    assert_eq!(
        store.list_terms(0, &key, u64::MAX)?,
        vec![Term::from(Literal::new_simple_literal("B"))]
    );
    Ok(())
}

/// A write with the subject's and predicate's UIDs and the object's encoding it resolved.
type Resolved<'a> = (
    QuadWrite<'a, SimStore, SimListCache<ultragraphdb_store_posting::CachedList>>,
    u64,
    u64,
    EncodedTerm,
);

/// A write over a request at the newest timestamp, the terms of `<s>`, `<p>` and `<o>` resolved: the subject's and
/// predicate's UIDs and the object's encoding.
fn resolved<'a>(store: &'a TestQuads, s: &str, p: &str, o: &str) -> TestResult<Resolved<'a>> {
    let mut w = QuadWrite::new(store, 0, store.lists().request(store.lists().applied_ts()));
    let (terms, quads) = w.terms();
    let (st, l) = (quads.store(), quads.leases());
    let subject = terms.iri(st, l, ex(s).as_str())?;
    let predicate = terms.iri(st, l, ex(p).as_str())?;
    let object = EncodedTerm::uid(terms.iri(st, l, ex(o).as_str())?);
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
    assert_eq!(w.holds(0, s, p, &o)?, Some(Vec::new()));
    let key = forward(&store, 0, 0, &ex("a"), &ex("knows"))?;
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
    assert_eq!(w.holds(0, s, p, &o)?, None);
    let key = forward(&store, 0, 0, &ex("a"), &ex("knows"))?;
    assert_eq!(
        w.request().tested().collect::<Vec<_>>(),
        vec![(key.as_slice(), o.as_bytes())]
    );
    assert_eq!(w.request().read_keys().count(), 0);
    Ok(())
}

/// The statement IDs of a held statement are those its posting carries.
#[test]
fn test_a_held_statement_carries_its_statement_ids() -> TestResult {
    let store = quads()?;
    let (mut w, s, p, o) = resolved(&store, "a", "knows", "b")?;
    w.insert(0, s, p, &o, &[7, 9], "iri");
    let _committed_at: u64 = commit(&store, vec![w])?.remove(0)?;
    let (mut reader, subject, predicate, object) = resolved(&store, "a", "knows", "b")?;
    assert_eq!(
        reader.holds(0, subject, predicate, &object)?,
        Some(vec![7, 9])
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
    assert_eq!(w.holds(0, s, p, &o)?, None);
    assert_eq!(w.request().tested().count(), 0);
    assert_eq!(w.request().read_keys().count(), 0);
    w.insert(0, s, p, &o, &[], "iri");
    assert_eq!(
        w.holds(0, s, p, &o)?,
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
        EncodedTerm::uid(terms.iri(quads.store(), quads.leases(), ex("d").as_str())?)
    };
    w.insert(0, s, p, &d, &[], "iri");
    assert_eq!(w.holds(0, s, p, &d)?, Some(Vec::new()));
    assert!(w.delete(0, s, p, &b, "iri")?.is_some());
    assert_eq!(w.holds(0, s, p, &b)?, None);
    Ok(())
}

/// A delete keeps its read of the subject's list, which it needs whole to tell whether the predicate stays in the
/// subject's predicate list.
#[test]
fn test_a_delete_reads_the_subjects_list_whole() -> TestResult {
    let store = quads()?;
    write(&store, 0, &[quad("a", "knows", ex("b"))])?;
    let (mut w, s, p, o) = resolved(&store, "a", "knows", "b")?;
    let _deleted: Option<Vec<u64>> = w.delete(0, s, p, &o, "iri")?;
    let key = forward(&store, 0, 0, &ex("a"), &ex("knows"))?;
    assert!(w.request().read_keys().any(|k| k == key.as_slice()));
    Ok(())
}

/// A write seeded with a finished write continues it: its terms resolve as they did, its deltas and its statistics are
/// the write's own, and the two commit as one write with one statistics delta per predicate (7.5).
#[test]
fn test_a_seeded_write_continues_the_write_it_was_seeded_with() -> TestResult {
    let store = quads()?;
    let iri = |w: &mut QuadWrite<'_, _, _>, name: &str| {
        let (terms, quads) = w.terms();
        terms.iri(quads.store(), quads.leases(), ex(name).as_str())
    };
    let read_ts = store.lists().applied_ts();
    let mut first = QuadWrite::new(&store, 0, store.lists().request(read_ts));
    let (s1, p) = (iri(&mut first, "s1")?, iri(&mut first, "p")?);
    first.insert(0, s1, p, &literal("a")?, &[], "x");
    let (request, terms) = first.finish();
    let writes = request.into_writes();

    let mut second = QuadWrite::new(&store, 0, store.lists().request(read_ts));
    second.seed(&writes, terms)?;
    assert_eq!(
        iri(&mut second, "s1")?,
        s1,
        "the seeded terms resolve as before"
    );
    let s2 = iri(&mut second, "s2")?;
    second.insert(0, s2, p, &literal("b")?, &[], "x");
    let _committed_at: u64 = commit(&store, vec![second])?.remove(0)?;
    let stats = store.statistics(0, 0, p, store.lists().applied_ts())?;
    assert_eq!(stats.statements(), 2, "both inserts counted, in one delta");
    assert_eq!(
        store
            .objects(
                &forward(&store, 0, 0, &ex("s1"), &ex("p"))?,
                store.lists().applied_ts()
            )?
            .len(),
        1,
        "the seeded insert is written"
    );
    Ok(())
}
