/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Replay (7.9): an execution rebuilt from its write-set record's sections writes what the original wrote, entry for
//! entry; a drop and a build's end replayed at their versions do what they did, a build restored from the declaration
//! its end's record names built again, and an end whose build is gone writes nothing.

use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_store_apply::{BuildEnd, Executed, RecordWrites};
use ultragraphdb_store_quads::{HASH_TO_STRING, IRI_TO_UID, UID_TO_IRI};
use ultragraphdb_traits_store::{Store, VersionlessEntry};

use crate::utils::changesets::{
    NS, TestChangesets, changeset, changesets, changesets_over, declaration, declare, ex,
    finish_builds, indexed, insert, term_token, uid, write,
};

/// A write-set record's sections 0 to 4, owned: what the transaction builder writes for `executed`.
struct Sections {
    lists: Vec<(Vec<u8>, Vec<u8>)>,
    dictionary: Vec<(Vec<u8>, Option<Vec<u8>>)>,
    statements: Vec<(Vec<u8>, Option<Vec<u8>>)>,
    catalog: Vec<(Vec<u8>, Option<Vec<u8>>)>,
    versionless: Vec<(Vec<u8>, Option<Vec<u8>>)>,
}

fn keyspace(key: &[u8]) -> Option<Keyspace> {
    key.first()
        .and_then(|b| Keyspace::from_byte(*b))
        .map(|(k, _)| k)
}

fn sections(executed: &Executed) -> Sections {
    let writes = executed.writes();
    let owned =
        |e: &ultragraphdb_traits_store::Entry| (e.key().to_vec(), e.value().map(<[u8]>::to_vec));
    let of = |k: Keyspace| {
        writes
            .entries()
            .iter()
            .filter(|e| keyspace(e.key()) == Some(k))
            .map(owned)
            .collect()
    };
    let mut lists: Vec<(Vec<u8>, Vec<u8>)> = writes
        .deltas()
        .iter()
        .map(|(k, d)| (k.clone(), d.encode()))
        .collect();
    lists.extend(
        writes
            .entries()
            .iter()
            .filter(|e| {
                !matches!(
                    keyspace(e.key()),
                    Some(Keyspace::Catalog | Keyspace::Statements)
                )
            })
            .map(|e| (e.key().to_vec(), e.value().unwrap_or_default().to_vec())),
    );
    let tagged = |dictionary: bool| {
        writes
            .versionless()
            .iter()
            .filter(|e| {
                matches!(
                    e.key().get(8),
                    Some(&IRI_TO_UID | &UID_TO_IRI | &HASH_TO_STRING)
                ) == dictionary
            })
            .map(|e: &VersionlessEntry| (e.key().to_vec(), e.value().map(<[u8]>::to_vec)))
            .collect()
    };
    Sections {
        lists,
        dictionary: tagged(true),
        statements: of(Keyspace::Statements),
        catalog: of(Keyspace::Catalog),
        versionless: tagged(false),
    }
}

/// The record writes `sections` hold, borrowed.
fn record_writes(s: &Sections) -> RecordWrites<'_> {
    fn pairs(v: &[(Vec<u8>, Option<Vec<u8>>)]) -> Vec<(&[u8], Option<&[u8]>)> {
        v.iter()
            .map(|(k, v)| (k.as_slice(), v.as_deref()))
            .collect()
    }
    RecordWrites::new(
        s.lists
            .iter()
            .map(|(k, v)| (k.as_slice(), v.as_slice()))
            .collect(),
        pairs(&s.dictionary),
        pairs(&s.statements),
        pairs(&s.catalog),
        pairs(&s.versionless),
    )
}

/// A store's keys with their versions printed, and its entries without versions.
type Stored = (Vec<(Vec<u8>, String)>, Vec<(Vec<u8>, Vec<u8>)>);

/// The store's keys at their versions, and its entries without versions but the leases', which a replay never writes:
/// leases are synced when they are taken.
fn stored(a: &TestChangesets) -> Stored {
    let contents = a.quads().store().contents();
    let keys = contents
        .keys()
        .iter()
        .map(|(k, v)| (k.clone(), format!("{v:?}")))
        .collect();
    let versionless = contents
        .versionless()
        .iter()
        .filter(|(k, _)| !matches!(k.get(8), Some(0x10 | 0x11)))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    (keys, versionless)
}

fn next_version(a: &TestChangesets) -> u64 {
    a.quads().lists().applied_ts() + 2
}

#[test]
fn test_a_replayed_execution_writes_what_the_original_wrote() {
    let original = changesets();
    let replica = changesets();
    for a in [&original, &replica] {
        write(a, &["<a> <name> \"apple\"", "<a> <knows> <b>"]);
        declare(a, "age", "single-valued `xsd:integer`");
    }
    let executed = original
        .execute(&changeset(vec![
            insert(
                "<c> <name> \"a string long enough to be stored out of line in the dictionary\"",
            ),
            insert("<c> <knows> <a>"),
            insert("<d> <age> 7"),
            crate::utils::changesets::delete("<a> <knows> <b>"),
        ]))
        .unwrap();
    let declared = original
        .execute(&changeset(vec![
            ultragraphdb_core_mapping::Change::Declare(
                ultragraphdb_core_mapping::Declaration::Predicate(declaration("knows", "an edge")),
            ),
        ]))
        .unwrap();
    let parts = [sections(&executed), sections(&declared)];
    let version = next_version(&original);
    original
        .apply_round(vec![(version, executed), (version + 2, declared)])
        .unwrap();
    let replayed: Vec<(u64, Executed)> = parts
        .iter()
        .zip([version, version + 2])
        .map(|(s, v)| (v, replica.prepare_replay(NS, record_writes(s)).unwrap()))
        .collect();
    replica.apply_round(replayed).unwrap();
    replica.forget_catalogs();
    assert_eq!(
        stored(&replica),
        stored(&original),
        "entry for entry, versions and metadata"
    );
    assert_eq!(replica.quads().lists().applied_ts(), version + 2);
}

#[test]
fn test_a_replayed_drop_drops_its_predicate_at_its_version() {
    let original = changesets();
    let replica = changesets();
    for a in [&original, &replica] {
        write(a, &["<a> <p> <b>", "<a> <q> <b>"]);
    }
    let p = uid(&original, "p");
    let version = next_version(&original);
    let drop = original
        .prepare_drop_predicate(NS, ex("p").as_ref())
        .unwrap();
    original.apply_round(vec![(version, drop)]).unwrap();
    let replayed = replica.prepare_replayed_drop(NS, p).unwrap();
    assert!(replayed.is_drop());
    replica.apply_round(vec![(version, replayed)]).unwrap();
    replica.forget_catalogs();
    assert_eq!(stored(&replica), stored(&original));
}

/// A build in flight when the node stopped: the reopened store holds the interim declaration, and the swap's record
/// names the new one; restored from it, the build is made again and swapped in at the swap's version.
#[test]
fn test_a_swap_replayed_after_a_restart_builds_again_from_its_declaration() {
    let a = changesets();
    a.sequence_builds();
    write(&a, &["<a> <name> \"big apple\""]);
    declare(&a, "name", "with `term`");
    finish_builds(&a);
    let end = a.take_build_end().unwrap();
    let ending = a.prepare_build_end(end).unwrap();
    let entry = ending.writes().entries()[0].clone();
    assert_eq!(
        ultragraphdb_core_catalog::decode_declaration(entry.key(), entry.value().unwrap()).unwrap(),
        ultragraphdb_core_mapping::Declaration::Predicate(declaration("name", "with `term`")),
        "the swap's record names the declaration it swaps in"
    );
    let store = a.quads().store().clone();
    drop(ending);
    drop(a);
    let b = changesets_over(store);
    assert!(!b.rebuilding());
    assert!(
        b.catalog(NS)
            .unwrap()
            .predicate(uid(&b, "name"))
            .unwrap()
            .tokenizers()
            .is_empty()
    );
    assert!(
        b.restore_build(NS, end, entry.key(), entry.value().unwrap())
            .unwrap()
    );
    assert!(b.rebuilding());
    let swap = b.prepare_build_end(end).unwrap();
    let version = next_version(&b);
    b.apply_round(vec![(version, swap)]).unwrap();
    assert!(!b.rebuilding());
    assert_eq!(
        indexed(&b, "name", &term_token("apple")),
        vec![uid(&b, "a")]
    );
    assert_eq!(
        b.catalog(NS)
            .unwrap()
            .predicate(uid(&b, "name"))
            .unwrap()
            .tokenizers(),
        ["term"]
    );
    assert_eq!(b.quads().lists().applied_ts(), version);
}

/// An undo replayed writes the old declaration back and removes every index key of the predicate the declaration
/// does not name: those of a write during the build, replayed before it.
#[test]
fn test_an_undo_replayed_restores_the_old_declaration_and_removes_the_rest() {
    let a = changesets();
    a.sequence_builds();
    write(&a, &["<a> <name> \"big apple\""]);
    declare(&a, "name", "with `term`");
    let during = a
        .execute(&changeset(vec![insert("<c> <name> \"pear\"")]))
        .unwrap();
    let during_sections = sections(&during);
    let undo = a.prepare_build_end(BuildEnd::Undo(record_of(&a))).unwrap();
    let entry = undo.writes().entries()[0].clone();
    let end = BuildEnd::Undo(record_of(&a));
    let store = a.quads().store().clone();
    drop(undo);
    drop(during);
    drop(a);
    let b = changesets_over(store);
    let version = next_version(&b);
    let replayed = b
        .prepare_replay(NS, record_writes(&during_sections))
        .unwrap();
    b.apply_round(vec![(version, replayed)]).unwrap();
    b.forget_catalogs();
    assert_eq!(
        indexed(&b, "name", &term_token("pear")),
        vec![uid(&b, "c")],
        "the write maintained the index"
    );
    assert!(
        b.restore_build(NS, end, entry.key(), entry.value().unwrap())
            .unwrap()
    );
    let undo = b.prepare_build_end(end).unwrap();
    b.apply_round(vec![(version + 2, undo)]).unwrap();
    assert!(!b.rebuilding());
    assert!(
        indexed(&b, "name", &term_token("pear")).is_empty(),
        "removed"
    );
    assert!(
        b.catalog(NS)
            .unwrap()
            .predicate(uid(&b, "name"))
            .unwrap()
            .tokenizers()
            .is_empty()
    );
}

/// The record number of the build `a` runs.
fn record_of(a: &TestChangesets) -> u64 {
    a.quads()
        .store()
        .rebuild_records()
        .unwrap()
        .first()
        .map(|r| r.id())
        .expect("a build runs")
}

/// An end whose predicate the store no longer declares is not restored: its build was cancelled.
#[test]
fn test_an_end_whose_predicate_is_gone_is_not_restored() {
    let a = changesets();
    a.sequence_builds();
    write(&a, &["<a> <name> \"big apple\""]);
    declare(&a, "name", "with `term`");
    finish_builds(&a);
    let end = a.take_build_end().unwrap();
    let entry = a.prepare_build_end(end).unwrap().writes().entries()[0].clone();
    let version = next_version(&a);
    let dropping = a.prepare_drop_predicate(NS, ex("name").as_ref()).unwrap();
    a.apply_round(vec![(version, dropping)]).unwrap();
    let store = a.quads().store().clone();
    drop(a);
    let b = changesets_over(store);
    assert!(
        !b.restore_build(NS, end, entry.key(), entry.value().unwrap())
            .unwrap()
    );
    assert!(!b.rebuilding());
}
