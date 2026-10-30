/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Execution and rounds: a changeset executed against the snapshot at the watermark writes nothing and sees no other
//! execution's writes; a round of executions is written as one store batch, each at its version, and installs what
//! they leave in round order; a round that breaks a rule of the store is refused whole.

use std::collections::BTreeSet;

use ultragraphdb_core_mapping::{Change, Declaration, PredicateDeclaration};
use ultragraphdb_store_apply::{ChangesetErrorEnum, Executed};
use ultragraphdb_store_posting::{PostingError, PostingErrorEnum};
use ultragraphdb_store_quads::iri_key;
use ultragraphdb_store_sim::OpRecord;
use ultragraphdb_traits_store::Store;

use crate::utils::changesets::{
    NS, TestChangesets, changeset, changesets, declaration, ex, finish_builds, insert,
    is_held_in_default, objects, term, uid, write,
};

/// The declaration a predicate takes when first written undeclared: of kind either, multi-valued, no datatype, not
/// unique, no index (the catalog specification's defaults), stated through the declaration's own constructor.
fn default_of(predicate: &str) -> PredicateDeclaration {
    PredicateDeclaration::new(ex(predicate))
}

fn execute(a: &TestChangesets, text: &str) -> Executed {
    a.execute(&changeset(vec![insert(text)]))
        .expect("the changeset executes")
}

#[test]
fn test_an_execution_writes_nothing_and_reads_at_the_watermark() {
    let a = changesets();
    write(&a, &["<a> <knows> <z>"]);
    let watermark = a.quads().lists().applied_ts();
    let before = a.quads().store().contents();
    let executed = execute(&a, "<a> <knows> <b>");
    assert_eq!(a.quads().store().contents(), before, "nothing is written");
    assert_eq!(executed.read_ts(), watermark);
    assert_eq!(executed.namespace(), NS);
    assert!(
        executed
            .writes()
            .written_keys()
            .any(|k| ultragraphdb_core_keys::Region::of(k).is_ok()),
        "its deltas are in its writes"
    );
    assert_eq!(executed.reads().namespace(), NS);
    assert_eq!(executed.reads().absent(), &[iri_key(NS, ex("b").as_str())]);
    assert_eq!(
        a.quads().lists().oldest_open_read(),
        None,
        "its read is closed"
    );
}

/// Two executions from one watermark do not see each other: each inserts a statement neither found held, so both
/// write it, and each gives a new IRI its own provisional UID.
#[test]
fn test_executions_from_one_watermark_do_not_see_each_other() {
    let a = changesets();
    write(&a, &["<a> <knows> <z>", "<b> <p> <q>"]);
    let first = execute(&a, "<a> <knows> <b>");
    let second = execute(&a, "<a> <knows> <b>");
    assert_eq!(first.writes().deltas(), second.writes().deltas());
    assert!(!first.writes().deltas().is_empty());
    let created = |e: &Executed| -> Vec<(String, u64)> {
        e.terms()
            .created()
            .map(|(i, u)| (i.to_string(), u))
            .collect()
    };
    let (one, two) = (
        execute(&a, "<n> <knows> <b>"),
        execute(&a, "<n> <knows> <b>"),
    );
    assert_eq!(created(&one).len(), 1);
    assert_eq!(created(&one)[0].0, ex("n").as_str());
    assert_ne!(
        created(&one)[0].1,
        created(&two)[0].1,
        "each takes its own provisional UID"
    );
    assert_eq!(one.reads().absent(), two.reads().absent());
}

/// A changeset executes on another thread while one is executed here: executions take no lock the commit step holds,
/// and an execution moves to the thread that commits it.
#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "the test's subject is executions on threads of their own, which a worker of the server gives them"
)]
fn test_executions_run_on_other_threads() {
    let a = changesets();
    write(&a, &["<a> <knows> <z>"]);
    let executed: Vec<Executed> = std::thread::scope(|scope| {
        let handles: Vec<_> = ["<a> <knows> <b>", "<a> <knows> <c>"]
            .iter()
            .map(|text| {
                let a = &a;
                scope.spawn(move || execute(a, text))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("executes"))
            .collect()
    });
    let next = a.quads().lists().next_version();
    let round: Vec<(u64, Executed)> = [next, next + 2].into_iter().zip(executed).collect();
    let applied = a.apply_round(round).expect("the round is written");
    assert_eq!(
        applied.iter().map(|x| x.commit_ts()).collect::<Vec<_>>(),
        vec![next, next + 2]
    );
    let held: BTreeSet<String> = objects(&a, "a", "knows")
        .iter()
        .map(|t| t.to_string())
        .collect();
    assert_eq!(held.len(), 3, "{held:?}");
}

/// A round is one store batch: every execution at its version, the last recorded as applied; each one's `Applied`
/// carries its version, the regions it wrote and read and its read set.
#[test]
fn test_a_round_writes_its_executions_at_their_versions_in_one_batch() {
    let a = changesets();
    write(&a, &["<a> <knows> <z>", "<b> <p> <q>"]);
    let executions = vec![execute(&a, "<a> <knows> <b>"), execute(&a, "<b> <p> <r>")];
    let reads: Vec<_> = executions.iter().map(|e| e.reads().clone()).collect();
    a.quads().store().take_log();
    let next = a.quads().lists().next_version();
    let round: Vec<(u64, Executed)> = [next + 2, next + 6].into_iter().zip(executions).collect();
    let applied = a.apply_round(round).expect("the round is written");
    let batches = a
        .quads()
        .store()
        .take_log()
        .into_iter()
        .filter(|r| matches!(r, OpRecord::Applied { .. }))
        .count();
    assert_eq!(batches, 1);
    assert_eq!(applied[0].commit_ts(), next + 2);
    assert_eq!(applied[1].commit_ts(), next + 6);
    assert_eq!(a.quads().lists().applied_ts(), next + 6);
    assert_eq!(applied[0].reads(), &reads[0]);
    assert!(!applied[0].written().is_empty() && !applied[0].read().is_empty());
    let mut held: Vec<String> = objects(&a, "b", "p")
        .iter()
        .map(|t| t.to_string())
        .collect();
    held.sort();
    assert_eq!(held, vec![term("<q>").to_string(), term("<r>").to_string()]);
    assert!(is_held_in_default(&a, "a", "knows"));
}

/// A term new to an execution takes a provisional UID, which its commit confirms: the dictionary then holds it.
#[test]
fn test_a_provisional_uid_is_confirmed_at_commit() {
    let a = changesets();
    let executed = execute(&a, "<new> <p> <q>");
    let provisional: Vec<(String, u64)> = executed
        .terms()
        .created()
        .map(|(i, u)| (i.to_string(), u))
        .collect();
    let new_uid = provisional
        .iter()
        .find(|(i, _)| i == ex("new").as_str())
        .expect("<new> is created")
        .1;
    assert_eq!(a.quads().uid_of(NS, ex("new").as_str()).unwrap(), None);
    let version = a.quads().lists().next_version();
    a.apply_round(vec![(version, executed)]).unwrap();
    assert_eq!(uid(&a, "new"), new_uid);
}

/// The catalog an execution leaves is installed by its round, not by its execution.
#[test]
fn test_a_round_installs_the_catalogs_its_executions_leave() {
    let a = changesets();
    write(&a, &["<a> <knows> <z>"]);
    let declared = declaration("tag", "a property");
    let executed = a
        .execute(&changeset(vec![Change::Declare(Declaration::Predicate(
            declared.clone(),
        ))]))
        .unwrap();
    let alias = execute(&a, "<a> <alias> \"x\"");
    let before = (*a.catalog(NS).unwrap()).clone();
    assert!(
        before
            .predicates()
            .all(|(_, d)| d.predicate().as_str() != ex("tag").as_str())
    );
    let next = a.quads().lists().next_version();
    a.apply_round(vec![(next, executed)]).unwrap();
    let catalog = a.catalog(NS).unwrap();
    assert_eq!(catalog.predicate(uid(&a, "tag")), Some(&declared));
    drop(alias);
    assert_eq!(
        a.quads().uid_of(NS, ex("alias").as_str()).unwrap(),
        None,
        "an execution never applied writes nothing"
    );
    let snapshot = a
        .quads()
        .store()
        .snapshot(a.quads().lists().applied_ts())
        .unwrap();
    let stored = ultragraphdb_core_catalog::Catalog::load(&snapshot, NS).unwrap();
    assert_eq!(*a.catalog(NS).unwrap(), stored);
}

#[test]
fn test_an_empty_round_writes_nothing() {
    let a = changesets();
    assert!(a.apply_round(Vec::new()).unwrap().is_empty());
    assert!(a.quads().store().contents().keys().is_empty());
}

/// A version at or below the watermark refuses the round whole, naming it, and nothing is written.
#[test]
fn test_a_round_at_a_stale_version_is_refused_whole() {
    let a = changesets();
    write(&a, &["<a> <knows> <z>"]);
    let watermark = a.quads().lists().applied_ts();
    let before = a.quads().store().contents();
    let error = a
        .apply_round(vec![(watermark, execute(&a, "<a> <knows> <b>"))])
        .unwrap_err();
    assert_eq!(
        error.0,
        ChangesetErrorEnum::Posting(PostingError(PostingErrorEnum::StaleVersion {
            version: watermark,
            after: watermark
        }))
    );
    assert_eq!(a.quads().store().contents(), before);
}

/// One build runs at a time: a round whose executions start two, or one while a build runs, is refused whole naming
/// the predicate, and writes nothing; once the build is done, the declaration is accepted.
#[test]
fn test_a_round_starting_a_second_build_is_refused_whole() {
    let a = changesets();
    write(&a, &["<a> <name> \"Ann\"", "<a> <tag> \"x\""]);
    let declare_change = |p: &str| {
        changeset(vec![Change::Declare(Declaration::Predicate(declaration(
            p,
            "with `exact`",
        )))])
    };
    let (name, tag) = (
        a.execute(&declare_change("name")).unwrap(),
        a.execute(&declare_change("tag")).unwrap(),
    );
    let before = a.quads().store().contents();
    let next = a.quads().lists().next_version();
    let refused = a
        .apply_round(vec![(next, name), (next + 2, tag)])
        .unwrap_err();
    assert_eq!(
        refused.to_string(),
        "An index is being built, so <http://ex.org/tag> cannot be declared now. Please retry"
    );
    assert_eq!(a.quads().store().contents(), before);
    assert!(!a.rebuilding());

    // A build started by one round refuses a second started while it runs, at execution or at its round.
    let tag = a.execute(&declare_change("tag")).unwrap();
    a.apply(&declare_change("name")).unwrap();
    assert!(a.rebuilding());
    let next = a.quads().lists().next_version();
    assert!(matches!(
        a.apply_round(vec![(next, tag)]).unwrap_err().0,
        ChangesetErrorEnum::Rebuilding { .. }
    ));
    assert!(matches!(
        a.execute(&declare_change("tag")).unwrap_err().0,
        ChangesetErrorEnum::Rebuilding { .. }
    ));
    finish_builds(&a);
    a.apply(&declare_change("tag")).unwrap();
    finish_builds(&a);
    assert_eq!(
        a.catalog(NS).unwrap().predicate(uid(&a, "tag")),
        Some(&declaration("tag", "with `exact`"))
    );
}

/// A build started by an execution that is not the first of its round starts at that execution's version: its built
/// lists are written complete there.
#[test]
fn test_a_build_starts_at_its_own_executions_version() {
    let a = changesets();
    write(&a, &["<a> <name> \"Ann\""]);
    let plain = execute(&a, "<b> <p> <q>");
    let declared = a
        .execute(&changeset(vec![Change::Declare(Declaration::Predicate(
            declaration("name", "with `exact`"),
        ))]))
        .unwrap();
    let next = a.quads().lists().next_version();
    a.apply_round(vec![(next, plain), (next + 4, declared)])
        .unwrap();
    finish_builds(&a);
    let key = ultragraphdb_core_keys::QuadKey::ValueIndex {
        namespace: NS,
        graph: ultragraphdb_core_keys::DEFAULT_GRAPH,
        predicate: uid(&a, "name"),
        tokenizer: crate::utils::changesets::exact_token("Ann").0,
        token: crate::utils::changesets::exact_token("Ann").1,
    }
    .to_bytes();
    let versions = a.quads().store().contents().versions(&key).to_vec();
    assert_eq!(versions.len(), 1, "{versions:?}");
    assert_eq!(versions[0].version(), next + 4);
}

/// A round at a stale version is refused before anything is written, the rebuild record of its declarations
/// included.
#[test]
fn test_a_stale_round_writes_no_rebuild_record() {
    let a = changesets();
    write(&a, &["<a> <name> \"Ann\""]);
    let declared = a
        .execute(&changeset(vec![Change::Declare(Declaration::Predicate(
            declaration("name", "with `exact`"),
        ))]))
        .unwrap();
    let before = a.quads().store().contents();
    let watermark = a.quads().lists().applied_ts();
    let refused = a.apply_round(vec![(watermark, declared)]).unwrap_err();
    // The version is the watermark itself, not above it.
    assert_eq!(
        refused.0,
        ChangesetErrorEnum::Posting(PostingError(PostingErrorEnum::StaleVersion {
            version: watermark,
            after: watermark
        }))
    );
    assert!(!a.rebuilding());
    assert_eq!(a.quads().store().contents(), before);
    assert!(a.quads().store().rebuild_records().unwrap().is_empty());
}

/// An execution names the catalog keys it writes as a predicate's implicit default entry, sorted, and no other: a
/// declaration's entry, even of a predicate the changeset first wrote, is a catalog change (decided 2026-10-27).
#[test]
fn test_an_execution_names_the_default_entries_it_writes() {
    let a = changesets();
    write(
        &a,
        &["<a> <knows> <z>", "<z> <seen> <score>", "<z> <seen> <age>"],
    );
    let first = a
        .execute(&changeset(vec![
            insert("<a> <score> \"x\""),
            insert("<a> <age> \"y\""),
            insert("<a> <knows> <b>"),
        ]))
        .unwrap();
    let mut expected = vec![
        ultragraphdb_core_catalog::Catalog::predicate_key(NS, uid(&a, "score")),
        ultragraphdb_core_catalog::Catalog::predicate_key(NS, uid(&a, "age")),
    ];
    expected.sort();
    assert_eq!(first.defaults(), expected.as_slice());
    let declared = a
        .execute(&changeset(vec![
            insert("<a> <score> \"x\""),
            Change::Declare(Declaration::Predicate(declaration("score", "a score"))),
        ]))
        .unwrap();
    assert!(declared.defaults().is_empty());
    let known = a
        .execute(&changeset(vec![insert("<a> <knows> <c>")]))
        .unwrap();
    assert!(known.defaults().is_empty());
}

/// Two executions from one watermark first write one predicate, the first another besides: once their round is
/// written, the catalog holds both predicates' default entries, as the store does, so the second execution's catalog
/// does not hide the first one's entry.
#[test]
fn test_a_round_keeps_the_default_entries_of_the_executions_before_it() {
    let a = changesets();
    write(
        &a,
        &["<a> <knows> <z>", "<z> <seen> <score>", "<z> <seen> <age>"],
    );
    let first = a
        .execute(&changeset(vec![
            insert("<a> <score> \"x\""),
            insert("<a> <age> \"y\""),
        ]))
        .unwrap();
    let second = a
        .execute(&changeset(vec![insert("<b> <score> \"z\"")]))
        .unwrap();
    let next = a.quads().lists().next_version();
    a.apply_round(vec![(next, first), (next + 2, second)])
        .unwrap();
    let catalog = a.catalog(NS).unwrap();
    // Each predicate first written holds the default declaration (the catalog specification's defaults).
    assert_eq!(catalog.predicate(uid(&a, "age")), Some(&default_of("age")));
    assert_eq!(catalog.predicate(uid(&a, "score")), Some(&default_of("score")));
    let snapshot = a
        .quads()
        .store()
        .snapshot(a.quads().lists().applied_ts())
        .unwrap();
    let stored = ultragraphdb_core_catalog::Catalog::load(&snapshot, NS).unwrap();
    assert_eq!(*catalog, stored);
}

/// An execution that changed the catalog, from the watermark another execution of its round first wrote a predicate
/// from, is installed with that predicate's default entry: the default entry did not abort it, so its catalog does
/// not hold it.
#[test]
fn test_a_catalog_change_keeps_the_default_entries_written_since_it_read() {
    let a = changesets();
    write(&a, &["<a> <knows> <z>", "<z> <seen> <age>"]);
    let first = a
        .execute(&changeset(vec![insert("<a> <age> \"y\"")]))
        .unwrap();
    let declared = a
        .execute(&changeset(vec![Change::Declare(Declaration::Predicate(
            declaration("tag", "a property"),
        ))]))
        .unwrap();
    let next = a.quads().lists().next_version();
    a.apply_round(vec![(next, first), (next + 2, declared)])
        .unwrap();
    let catalog = a.catalog(NS).unwrap();
    assert_eq!(catalog.predicate(uid(&a, "age")), Some(&default_of("age")));
    assert_eq!(
        catalog.predicate(uid(&a, "tag")),
        Some(&declaration("tag", "a property"))
    );
    let snapshot = a
        .quads()
        .store()
        .snapshot(a.quads().lists().applied_ts())
        .unwrap();
    assert_eq!(
        *catalog,
        ultragraphdb_core_catalog::Catalog::load(&snapshot, NS).unwrap()
    );
}

/// An execution that wrote no catalog entry leaves the catalog as the executions before it in its round left it.
#[test]
fn test_an_execution_writing_no_catalog_entry_leaves_the_catalog() {
    let a = changesets();
    write(&a, &["<a> <knows> <z>", "<z> <seen> <age>"]);
    let first = a
        .execute(&changeset(vec![insert("<a> <age> \"y\"")]))
        .unwrap();
    let plain = a
        .execute(&changeset(vec![insert("<a> <knows> <c>")]))
        .unwrap();
    let next = a.quads().lists().next_version();
    a.apply_round(vec![(next, first), (next + 2, plain)])
        .unwrap();
    let catalog = a.catalog(NS).unwrap();
    assert_eq!(catalog.predicate(uid(&a, "age")), Some(&default_of("age")));
    let snapshot = a
        .quads()
        .store()
        .snapshot(a.quads().lists().applied_ts())
        .unwrap();
    assert_eq!(
        *catalog,
        ultragraphdb_core_catalog::Catalog::load(&snapshot, NS).unwrap()
    );
}

/// The catalog installed is the one the store holds whatever the round's order: a default entry written after a
/// declaration of its predicate in one round, which the resolver never admits together since the declaration is a
/// catalog change, lands in both as the store's batch writes it, the later write.
#[test]
fn test_the_installed_catalog_is_the_stored_one_after_a_default_follows_a_declaration() {
    let a = changesets();
    write(&a, &["<a> <knows> <z>", "<z> <seen> <score>"]);
    let declared = a
        .execute(&changeset(vec![Change::Declare(Declaration::Predicate(
            declaration("score", "a property"),
        ))]))
        .unwrap();
    let default = a
        .execute(&changeset(vec![insert("<a> <score> \"y\"")]))
        .unwrap();
    let next = a.quads().lists().next_version();
    a.apply_round(vec![(next, declared), (next + 2, default)])
        .unwrap();
    let snapshot = a
        .quads()
        .store()
        .snapshot(a.quads().lists().applied_ts())
        .unwrap();
    assert_eq!(
        *a.catalog(NS).unwrap(),
        ultragraphdb_core_catalog::Catalog::load(&snapshot, NS).unwrap()
    );
}
