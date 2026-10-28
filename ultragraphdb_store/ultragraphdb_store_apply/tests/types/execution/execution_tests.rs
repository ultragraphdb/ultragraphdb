/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! An execution held open across a transaction's requests: each changeset applied on top of the ones before, reads
//! of its own writes over its snapshot, recorded in its read set.

use oxrdf::{GraphName, Term};
use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey};
use ultragraphdb_store_quads::iri_key;

use crate::utils::changesets::{NS, TestChangesets, changeset, changesets, ex, insert, uid};

fn read(
    execution: &mut ultragraphdb_store_apply::Execution<
        '_,
        ultragraphdb_store_sim::SimStore,
        ultragraphdb_store_sim::SimListCache<ultragraphdb_store_posting::CachedList>,
    >,
    subject: &str,
    predicate: &str,
) -> Vec<Term> {
    let mut out = vec![Term::from(ex("stale"))];
    let (s, p) = (ex(subject), ex(predicate));
    execution
        .objects_into(
            GraphName::DefaultGraph.as_ref(),
            s.as_ref().into(),
            p.as_ref(),
            &mut out,
        )
        .expect("the read");
    out.sort_by_key(Term::to_string);
    out
}

fn commit(applier: &TestChangesets, text: &str) -> u64 {
    applier
        .apply(&changeset(vec![insert(text)]))
        .expect("applied")
        .commit_ts()
}

#[test]
fn test_an_execution_reads_its_own_writes_over_its_snapshot() {
    let applier = changesets();
    let at = commit(&applier, "<a> <knows> <z>");
    let mut execution = applier.begin(NS, at).expect("begun");
    assert_eq!((execution.namespace(), execution.read_ts()), (NS, at));
    execution
        .apply(&changeset(vec![insert("<a> <knows> <b>")]))
        .unwrap();
    execution
        .apply(&changeset(vec![insert("<n> <knows> <a>")]))
        .unwrap();
    assert_eq!(
        read(&mut execution, "a", "knows"),
        vec![Term::from(ex("b")), Term::from(ex("z"))]
    );
    // A subject the execution created reads its delta.
    assert_eq!(
        read(&mut execution, "n", "knows"),
        vec![Term::from(ex("a"))]
    );
    let executed = execution.finish();
    let forward = QuadKey::Forward {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: uid(&applier, "knows"),
        subject: uid(&applier, "a"),
    }
    .to_bytes();
    assert!(
        executed.reads().lists().contains(&forward),
        "the read is a whole-list read"
    );
    assert_eq!(executed.read_ts(), at);
}

#[test]
fn test_an_execution_at_an_earlier_version_reads_that_snapshot() {
    let applier = changesets();
    let first = commit(&applier, "<a> <knows> <b>");
    commit(&applier, "<a> <knows> <c>");
    let mut execution = applier.begin(NS, first).expect("begun");
    assert_eq!(
        read(&mut execution, "a", "knows"),
        vec![Term::from(ex("b"))]
    );
}

#[test]
fn test_a_read_of_an_unknown_subject_and_predicate_records_the_subject_absent() {
    // Neither term exists: the subject is looked up first, so its absence is what a later creation of it conflicts on.
    let applier = changesets();
    let at = commit(&applier, "<a> <knows> <b>");
    let mut execution = applier.begin(NS, at).expect("begun");
    assert!(read(&mut execution, "ghost", "phantom").is_empty());
    let executed = execution.finish();
    assert!(
        executed
            .reads()
            .absent()
            .contains(&iri_key(NS, ex("ghost").as_str()))
    );
}

#[test]
fn test_a_read_of_unknown_terms_holds_nothing_and_records_them_absent() {
    let applier = changesets();
    let at = commit(&applier, "<a> <knows> <b>");
    let mut execution = applier.begin(NS, at).expect("begun");
    assert!(read(&mut execution, "nobody", "knows").is_empty());
    assert!(read(&mut execution, "a", "never").is_empty());
    let mut out = vec![Term::from(ex("stale"))];
    let (s, p, g) = (ex("a"), ex("knows"), ex("nograph"));
    execution
        .objects_into(
            GraphName::NamedNode(g.clone()).as_ref(),
            s.as_ref().into(),
            p.as_ref(),
            &mut out,
        )
        .unwrap();
    assert!(out.is_empty());
    let executed = execution.finish();
    for name in ["nobody", "never", "nograph"] {
        assert!(
            executed
                .reads()
                .absent()
                .contains(&iri_key(NS, ex(name).as_str())),
            "{name} recorded absent"
        );
    }
}
