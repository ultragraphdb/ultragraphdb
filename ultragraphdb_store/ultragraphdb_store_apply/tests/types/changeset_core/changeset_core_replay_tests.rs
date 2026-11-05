/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Replay (7.9): an execution rebuilt from its write-set record's sections writes what the original wrote, entry for
//! entry; a drop and a build's end replayed at their versions do what they did, a build restored from the declaration
//! its end's record names built again, and an end whose build is gone writes nothing.

use ultragraphdb_core_catalog::CatalogContents;
use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_core_mapping::Declaration;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_apply::{Applied, ChangesetErrorEnum, Executed, RecordWrites};
use ultragraphdb_traits_store::{Store, VersionlessEntry};

use crate::utils::changesets::{
    TENANT, TestChangesets, changeset, changesets, changesets_over, declaration, declare, ex,
    finish_builds, indexed, insert, term_token, uid, write,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// A write-set record's sections, owned: what the transaction builder writes for `executed`.
struct Sections {
    lists: Vec<(Vec<u8>, Vec<u8>)>,
    dictionary: Vec<(Vec<u8>, Option<Vec<u8>>)>,
    statements: Vec<(Vec<u8>, Option<Vec<u8>>)>,
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
            .filter(|e| !matches!(keyspace(e.key()), Some(Keyspace::Statements)))
            .map(|e| (e.key().to_vec(), e.value().unwrap_or_default().to_vec())),
    );
    let tagged = |dictionary: bool| {
        writes
            .versionless()
            .iter()
            .filter(|e| (keyspace(e.key()) == Some(Keyspace::Dictionary)) == dictionary)
            .map(|e: &VersionlessEntry| (e.key().to_vec(), e.value().map(<[u8]>::to_vec)))
            .collect()
    };
    Sections {
        lists,
        dictionary: tagged(true),
        statements: of(Keyspace::Statements),
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
fn test_a_replayed_execution_writes_what_the_original_wrote() -> TestResult {
    let original = changesets()?;
    let replica = changesets()?;
    for a in [&original, &replica] {
        write(a, &["<a> <name> \"apple\"", "<a> <knows> <b>"])?;
        declare(a, "age", "single-valued `xsd:integer`")?;
    }
    let executed = original.execute(&changeset(vec![
        insert("<c> <name> \"a string long enough to be stored out of line in the dictionary\"")?,
        insert("<c> <knows> <a>")?,
        insert("<d> <age> 7")?,
        crate::utils::changesets::delete("<a> <knows> <b>")?,
    ]))?;
    let declared = original.execute(&changeset(vec![
        ultragraphdb_core_mapping::Change::Declare(
            ultragraphdb_core_mapping::Declaration::Predicate(declaration("knows", "an edge")?),
        ),
    ]))?;
    let parts = [sections(&executed), sections(&declared)];
    let version = next_version(&original);
    let _applied: Vec<Applied> =
        original.apply_round(vec![(version, executed), (version + 2, declared)])?;
    let replayed: Vec<(u64, Executed)> = parts
        .iter()
        .zip([version, version + 2])
        .map(|(s, v)| Ok((v, replica.prepare_replay(TENANT, record_writes(s))?)))
        .collect::<TestResult<_>>()?;
    let _replayed: Vec<Applied> = replica.apply_round(replayed)?;
    replica.forget_catalogs();
    assert_eq!(
        stored(&replica),
        stored(&original),
        "entry for entry, versions and metadata"
    );
    assert_eq!(replica.quads().lists().applied_ts(), version + 2);
    Ok(())
}

#[test]
fn test_a_replayed_drop_drops_its_predicate_at_its_version() -> TestResult {
    let original = changesets()?;
    let replica = changesets()?;
    for a in [&original, &replica] {
        write(a, &["<a> <p> <b>", "<a> <q> <b>"])?;
    }
    let p = uid(&original, "p")?;
    let version = next_version(&original);
    let drop = original.prepare_drop(
        TENANT,
        ultragraphdb_store_apply::DropRequest::Predicate(ex("p").as_ref()),
    )?;
    let _applied: Vec<Applied> = original.apply_round(vec![(version, drop)])?;
    let replayed = replica
        .prepare_replayed_drop(TENANT, ultragraphdb_store_apply::DropTarget::Predicate(p))?;
    assert!(replayed.is_drop());
    let _replayed: Vec<Applied> = replica.apply_round(vec![(version, replayed)])?;
    replica.forget_catalogs();
    assert_eq!(stored(&replica), stored(&original));
    Ok(())
}

/// The version `version` of the root tenant's catalog declaring `name` as `phrase`, or nothing for `None`.
fn name_version(phrase: Option<&str>) -> TestResult<CatalogContents> {
    let declarations = match phrase {
        Some(p) => vec![Declaration::Predicate(declaration("name", p)?)],
        None => Vec::new(),
    };
    Ok(CatalogContents::new(TENANT, 2, declarations))
}

/// A build in flight when the node stopped, its swap never applied: the reopened partition loads the version declaring
/// the index and builds it again from the data, the catalog holding the version's declaration once it is swapped in.
#[test]
fn test_a_build_in_flight_at_a_restart_is_built_again_from_its_version() -> TestResult {
    let a = changesets()?;
    a.sequence_builds();
    write(&a, &["<a> <name> \"big apple\""])?;
    declare(&a, "name", "with `term`")?;
    finish_builds(&a);
    let _end = a.take_build_end().context("a build ended")?;
    let store = a.quads().store().clone();
    drop(a);
    let b = changesets_over(store)?;
    assert!(!b.rebuilding());
    b.load_catalog(&name_version(Some("with `term`"))?)?;
    b.recover_builds()?;
    assert!(b.rebuilding(), "the build runs again");
    finish_builds(&b);
    assert!(!b.rebuilding());
    assert_eq!(
        indexed(&b, "name", &term_token("apple"))?,
        vec![uid(&b, "a")?]
    );
    assert_eq!(
        b.catalog(TENANT)?
            .predicate(uid(&b, "name")?)
            .context("the predicate is declared")?
            .tokenizers(),
        ["term"]
    );
    Ok(())
}

/// A build that failed, its predicate restored by the metadata group's next version: the reopened partition loads the
/// restoring version and drops every index key of the predicate it does not declare, a write's during the build too.
#[test]
fn test_a_restoring_version_loaded_after_a_restart_removes_the_built_index() -> TestResult {
    let a = changesets()?;
    a.sequence_builds();
    write(&a, &["<a> <name> \"big apple\""])?;
    declare(&a, "name", "with `term`")?;
    let during = a.execute(&changeset(vec![insert("<c> <name> \"pear\"")?]))?;
    let version = next_version(&a);
    let _applied: Vec<Applied> = a.apply_round(vec![(version, during)])?;
    assert_eq!(
        indexed(&a, "name", &term_token("pear"))?,
        vec![uid(&a, "c")?],
        "the write maintained the index"
    );
    let store = a.quads().store().clone();
    drop(a);
    let b = changesets_over(store)?;
    b.load_catalog(&name_version(None)?)?;
    b.recover_builds()?;
    assert!(!b.rebuilding());
    assert!(
        indexed(&b, "name", &term_token("pear"))?.is_empty(),
        "removed"
    );
    assert!(b.catalog(TENANT)?.predicate(uid(&b, "name")?).is_none());
    assert!(b.quads().store().rebuild_records()?.is_empty());
    Ok(())
}

/// A build a drop of its predicate cancelled leaves nothing to build again after a restart.
#[test]
fn test_a_build_whose_predicate_was_dropped_is_not_built_again() -> TestResult {
    let a = changesets()?;
    a.sequence_builds();
    write(&a, &["<a> <name> \"big apple\""])?;
    declare(&a, "name", "with `term`")?;
    finish_builds(&a);
    let _end = a.take_build_end().context("a build ended")?;
    let version = next_version(&a);
    let dropping = a.prepare_drop(
        TENANT,
        ultragraphdb_store_apply::DropRequest::Predicate(ex("name").as_ref()),
    )?;
    let _applied: Vec<Applied> = a.apply_round(vec![(version, dropping)])?;
    let store = a.quads().store().clone();
    drop(a);
    let b = changesets_over(store)?;
    b.load_catalog(&name_version(None)?)?;
    b.recover_builds()?;
    assert!(!b.rebuilding());
    Ok(())
}

/// A record whose list delta is no posting list is refused with the store's corruption naming the key, and nothing is
/// prepared: the posting codec's version byte is 1 (`ultragraphdb_core_codec`), so a value starting 0xFF is of an
/// unknown version.
#[test]
fn test_a_replayed_list_delta_that_is_no_posting_list_is_refused_naming_its_key() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <knows> <b>"])?;
    let key = ultragraphdb_core_keys::QuadKey::Forward {
        tenant: TENANT,
        graph: ultragraphdb_core_keys::DEFAULT_GRAPH,
        predicate: uid(&a, "knows")?,
        subject: uid(&a, "a")?,
    }
    .to_bytes();
    let record = RecordWrites::new(
        vec![(key.as_slice(), &[0xFF, 0, 0][..])],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    let refused = a
        .prepare_replay(TENANT, record)
        .err()
        .context("the error of a.prepare_replay(TENANT, record)")?;
    assert_eq!(
        refused.0,
        ChangesetErrorEnum::Store(StoreError::corrupted(
            key.clone(),
            "unknown posting format version 255".to_string()
        ))
    );
    Ok(())
}

/// A replayed record carrying statement records writes them at its version, and one removing a statement's record
/// removes it: the replica holds what the original holds after each.
#[test]
fn test_a_replayed_record_writes_and_removes_statement_records() -> TestResult {
    let original = changesets()?;
    let replica = changesets()?;
    let inserted = original.execute(&changeset(vec![crate::utils::changesets::insert_edge(
        "<a> <knows> <b>",
        &[("since", "2020")],
    )?]))?;
    let first = sections(&inserted);
    assert!(
        !first.statements.is_empty() && first.statements.iter().all(|(_, v)| v.is_some()),
        "the insert writes its statement's record: {:?}",
        first.statements
    );
    let version = next_version(&original);
    let _applied: Vec<Applied> = original.apply_round(vec![(version, inserted)])?;
    let replayed = replica.prepare_replay(TENANT, record_writes(&first))?;
    let _replayed: Vec<Applied> = replica.apply_round(vec![(version, replayed)])?;
    assert_eq!(stored(&replica), stored(&original), "after the insert");

    let deleted = original.execute(&changeset(vec![crate::utils::changesets::delete(
        "<a> <knows> <b>",
    )?]))?;
    let second = sections(&deleted);
    assert!(
        !second.statements.is_empty() && second.statements.iter().all(|(_, v)| v.is_none()),
        "the delete removes the statement's record: {:?}",
        second.statements
    );
    let delete_version = next_version(&original);
    let _deleted: Vec<Applied> = original.apply_round(vec![(delete_version, deleted)])?;
    let replayed_delete = replica.prepare_replay(TENANT, record_writes(&second))?;
    let _replayed_delete: Vec<Applied> =
        replica.apply_round(vec![(delete_version, replayed_delete)])?;
    assert_eq!(stored(&replica), stored(&original), "after the delete");
    Ok(())
}
