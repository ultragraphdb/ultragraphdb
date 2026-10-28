/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Drops at their edges: a predicate the store never named, a predicate declared without data, a predicate in several
//! graphs with its index keys and statement records, graphs the store does not hold, a namespace never written, a
//! drop during a build; and the dictionary every drop keeps.

use oxrdf::{BlankNode, GraphName, GraphNameRef};
use ultragraphdb_core_keys::{Keyspace, QuadKey};

use ultragraphdb_traits_store::Store;

use crate::utils::changesets::{
    NS, TestChangesets, apply, changesets, declaration, declare, ex, finish_builds, indexed,
    insert, insert_edge, keys_of, objects_in, predicate_list, stored_keys, term, term_token, uid,
    write,
};

fn names(a: &TestChangesets, predicate: u64) -> Vec<Vec<u8>> {
    stored_keys(a)
        .into_iter()
        .filter(|k| QuadKey::parse(k).is_ok_and(|(q, _)| q.predicate() == Some(predicate)))
        .collect()
}

#[test]
fn test_dropping_a_predicate_the_store_never_named_changes_nothing() {
    let a = changesets();
    write(&a, &["<a> <knows> <b>"]);
    let before = a.quads().store().contents();
    let latest = a.quads().lists().applied_ts();
    assert_eq!(a.drop_predicate(NS, ex("never").as_ref()).unwrap(), latest);
    assert_eq!(a.quads().store().contents(), before);
}

#[test]
fn test_dropping_a_predicate_without_data_removes_its_declaration() {
    let a = changesets();
    declare(&a, "name", "with `exact`");
    let p = uid(&a, "name");
    a.drop_predicate(NS, ex("name").as_ref()).unwrap();
    assert_eq!(a.catalog(NS).unwrap().predicate(p), None);
    assert!(names(&a, p).is_empty());
    // The dictionary keeps its UID, so a second drop finds it and drops nothing.
    assert_eq!(uid(&a, "name"), p);
    a.drop_predicate(NS, ex("name").as_ref()).unwrap();
    assert!(names(&a, p).is_empty());
}

#[test]
fn test_dropping_a_predicate_removes_it_from_every_graph_and_keyspace() {
    let a = changesets();
    declare(&a, "friend", "with an exact count index and `term`");
    apply(
        &a,
        vec![
            insert("<a> <friend> <b>"),
            insert("<a> <friend> \"best\" <g1>"),
            insert("<a> <name> \"Ann\" <g1>"),
            insert_edge("<c> <friend> <b> <g1>", &[("since", "2020")]),
        ],
    )
    .unwrap();
    let friend = uid(&a, "friend");
    for keyspace in [
        Keyspace::Forward,
        Keyspace::Reverse,
        Keyspace::ValueIndex,
        Keyspace::CountIndex,
        Keyspace::Statistics,
    ] {
        assert!(
            !keys_of(&a, keyspace, "friend").is_empty(),
            "{keyspace:?} before"
        );
    }
    let versionless = a.quads().store().contents().versionless().clone();
    a.drop_predicate(NS, ex("friend").as_ref()).unwrap();
    assert!(names(&a, friend).is_empty(), "{:?}", names(&a, friend));
    assert!(
        !stored_keys(&a)
            .iter()
            .any(|k| matches!(Keyspace::from_byte(k[0]), Some((Keyspace::Statements, _)))),
        "the record of <c> <friend> <b> in <g1> is gone"
    );
    assert!(predicate_list(&a, "a", false).is_empty());
    assert!(predicate_list(&a, "b", true).is_empty());
    assert_eq!(objects_in(&a, "g1", "a", "name"), vec![term("\"Ann\"")]);
    assert_eq!(a.catalog(NS).unwrap().predicate(friend), None);
    // The dictionary is kept: every versionless entry before the drop is still there.
    let after = a.quads().store().contents().versionless().clone();
    assert!(versionless.iter().all(|(k, v)| after.get(k) == Some(v)));
}

#[test]
fn test_dropping_a_graph_the_store_does_not_hold_changes_nothing() {
    let a = changesets();
    write(&a, &["<a> <knows> <b> <g1>"]);
    let before = a.quads().store().contents();
    let latest = a.quads().lists().applied_ts();
    assert_eq!(
        a.drop_graph(NS, GraphName::from(ex("never")).as_ref())
            .unwrap(),
        latest
    );
    let blank = BlankNode::new_unchecked("g");
    assert_eq!(
        a.drop_graph(NS, GraphNameRef::BlankNode(blank.as_ref()))
            .unwrap(),
        latest
    );
    assert_eq!(a.quads().store().contents(), before);
}

#[test]
fn test_dropping_a_graph_removes_its_index_keys_and_keeps_the_other_graphs() {
    let a = changesets();
    declare(&a, "name", "with `term`");
    declare(&a, "friend", "with an exact count index");
    write(
        &a,
        &[
            "<a> <name> \"big apple\" <g1>",
            "<a> <friend> <b> <g1>",
            "<a> <name> \"big apple\"",
            "<a> <friend> <b>",
        ],
    );
    let g1 = uid(&a, "g1");
    a.drop_graph(NS, GraphName::from(ex("g1")).as_ref())
        .unwrap();
    let left: Vec<Vec<u8>> = stored_keys(&a)
        .into_iter()
        .filter(|k| QuadKey::parse(k).is_ok_and(|(q, _)| q.graph() == Some(g1)))
        .collect();
    assert!(left.is_empty(), "{left:?}");
    assert_eq!(indexed(&a, "name", &term_token("big")), vec![uid(&a, "a")]);
    assert_eq!(objects_in(&a, "", "a", "friend"), vec![term("<b>")]);
}

#[test]
fn test_dropping_the_default_graph_keeps_the_named_ones() {
    let a = changesets();
    write(&a, &["<a> <knows> <b>", "<a> <knows> <c> <g1>"]);
    a.drop_graph(NS, GraphNameRef::DefaultGraph).unwrap();
    assert!(objects_in(&a, "", "a", "knows").is_empty());
    assert_eq!(objects_in(&a, "g1", "a", "knows"), vec![term("<c>")]);
}

#[test]
fn test_dropping_a_namespace_never_written_writes_no_catalog() {
    let a = changesets();
    write(&a, &["<a> <knows> <b>"]);
    let before = a.quads().store().contents();
    a.drop_namespace(5).unwrap();
    assert_eq!(a.quads().store().contents().keys(), before.keys());
    assert!(a.catalog(5).unwrap().predicates().next().is_none());
}

#[test]
fn test_dropping_the_predicate_being_built_cancels_its_build() {
    let a = changesets();
    declare(&a, "name", "with `term`");
    write(&a, &["<a> <name> \"big apple\""]);
    declare(&a, "name", "with `exact`");
    assert!(a.rebuilding());
    let p = uid(&a, "name");
    a.drop_predicate(NS, ex("name").as_ref()).unwrap();
    assert!(!a.rebuilding());
    finish_builds(&a);
    assert!(names(&a, p).is_empty(), "{:?}", names(&a, p));
    assert_eq!(a.catalog(NS).unwrap().predicate(p), None);
    assert!(a.quads().store().rebuild_records().unwrap().is_empty());
}

#[test]
fn test_dropping_the_data_during_a_build_declares_the_new_declaration() {
    let a = changesets();
    declare(&a, "name", "with `term`");
    write(&a, &["<a> <name> \"big apple\""]);
    declare(&a, "name", "with `exact`");
    a.drop_data(NS).unwrap();
    assert!(!a.rebuilding());
    finish_builds(&a);
    let p = uid(&a, "name");
    assert_eq!(
        a.catalog(NS).unwrap().predicate(p),
        Some(&declaration("name", "with `exact`"))
    );
    assert!(a.quads().store().rebuild_records().unwrap().is_empty());
    // Writes after the drop are indexed under the new declaration alone.
    write(&a, &["<b> <name> \"pear\""]);
    assert!(indexed(&a, "name", &term_token("pear")).is_empty());
    assert_eq!(keys_of(&a, Keyspace::ValueIndex, "name").len(), 1);
}

#[test]
fn test_each_drop_returns_its_own_timestamp() {
    let a = changesets();
    write(&a, &["<a> <knows> <b>", "<a> <knows> <c> <g1>"]);
    let drops: [&dyn Fn() -> u64; 5] = [
        &|| a.drop_predicate(NS, ex("knows").as_ref()).unwrap(),
        &|| {
            a.drop_graph(NS, GraphName::from(ex("g1")).as_ref())
                .unwrap()
        },
        &|| a.drop_data(NS).unwrap(),
        &|| a.drop_namespace(NS).unwrap(),
        &|| a.drop_all().unwrap(),
    ];
    for (i, drop) in drops.iter().enumerate() {
        write(&a, &["<a> <knows> <b>", "<a> <knows> <c> <g1>"]);
        let before = a.quads().lists().applied_ts();
        let ts = drop();
        assert!(ts > before, "drop {i}: {ts} after {before}");
        assert_eq!(ts, a.quads().lists().applied_ts(), "drop {i}");
    }
}

#[test]
fn test_dropping_another_namespace_keeps_the_build() {
    let a = changesets();
    write(&a, &["<a> <name> \"pear\""]);
    a.apply(&ultragraphdb_core_mapping::Changeset::new(3).with(insert("<a> <name> \"fig\"")))
        .unwrap();
    declare(&a, "name", "with `exact`");
    a.drop_namespace(3).unwrap();
    a.drop_data(3).unwrap();
    assert!(a.rebuilding());
    finish_builds(&a);
    assert_eq!(
        indexed(&a, "name", &crate::utils::changesets::exact_token("pear")),
        vec![uid(&a, "a")]
    );
}

#[test]
fn test_dropping_a_predicate_rewrites_a_split_predicate_list_whole() {
    use ultragraphdb_core_keys::{DEFAULT_GRAPH, SPLIT_FLAG};
    use ultragraphdb_store_posting::{Lists, ListsConfig};
    use ultragraphdb_store_sim::{Seed, SimListCache, SteppedExecutor, Stream};
    use ultragraphdb_traits_store::PersistMode;
    // A list size of 256 bytes, as the split scenario's, so a subject holding 2,000 predicates splits.
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    let executor = SteppedExecutor::new(Seed::new(1).stream(Stream::Executor));
    let lists = Lists::new(
        crate::utils::changesets::sim_store(),
        cache,
        ListsConfig::new(PersistMode::Buffer, 256),
    );
    let a = TestChangesets::new(lists, executor, 1).unwrap();
    let changes = (0..2_000)
        .map(|i| insert(&format!("<a> <p{i}> <o>")))
        .collect();
    apply(&a, changes).unwrap();
    let key = QuadKey::SubjectPredicates {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        subject: uid(&a, "a"),
    }
    .to_bytes();
    a.quads().lists().rollup_key(&key).unwrap();
    let latest = a.quads().lists().applied_ts();
    // The rollup is written one above the commit it folds, past the watermark: a read of everything reaches it.
    assert!(
        !a.quads()
            .lists()
            .read(&key, u64::MAX)
            .unwrap()
            .plist()
            .splits()
            .is_empty(),
        "the list is split"
    );
    a.drop_predicate(NS, ex("p0").as_ref()).unwrap();
    // The list's parts, every version, were dropped and written again at the drop's read timestamp.
    let mut parts_prefix = key.clone();
    parts_prefix[0] |= SPLIT_FLAG;
    let contents = a.quads().store().contents();
    let parts: Vec<_> = contents
        .keys()
        .iter()
        .filter(|(k, _)| k.starts_with(&parts_prefix))
        .collect();
    assert!(!parts.is_empty());
    for (k, versions) in parts {
        assert!(
            versions.iter().all(|v| v.version() >= latest),
            "{k:?}: {:?}",
            versions.iter().map(|v| v.version()).collect::<Vec<_>>()
        );
    }
    let held = predicate_list(&a, "a", false);
    assert_eq!(held.len(), 1_999);
    assert!(!held.contains(&uid(&a, "p0")));
}

/// A drop prepared for the commit step writes the predicate's key ranges, its catalog entry's among them, and nothing
/// else; applied in a round at its version, it drops the predicate, and a predicate the store does not know is no drop
/// (7.7).
#[test]
fn test_a_drop_prepared_for_a_round_writes_its_ranges_and_drops_at_its_version() {
    use ultragraphdb_core_catalog::Catalog;
    let a = crate::utils::changesets::changesets();
    crate::utils::changesets::write(&a, &["<a> <p> <b>", "<a> <q> <b>"]);
    let unknown = a
        .prepare_drop_predicate(NS, crate::utils::changesets::ex("none").as_ref())
        .unwrap();
    assert!(!unknown.is_drop());
    assert!(unknown.drop_ranges().is_empty());
    let p = crate::utils::changesets::uid(&a, "p");
    let drop = a
        .prepare_drop_predicate(NS, crate::utils::changesets::ex("p").as_ref())
        .unwrap();
    assert!(drop.is_drop());
    assert!(!drop.starts_build());
    let catalog = Catalog::predicate_key(NS, p);
    assert!(
        drop.drop_ranges()
            .iter()
            .any(|(start, end)| start == &catalog && end.as_slice() > catalog.as_slice()),
        "the catalog entry's range"
    );
    assert!(drop.drop_ranges().iter().all(|(s, e)| s < e));
    assert!(drop.writes().deltas().is_empty());
    let version = a.quads().lists().applied_ts() + 2;
    a.apply_round(vec![(version, drop)]).unwrap();
    assert_eq!(a.quads().lists().applied_ts(), version);
    assert!(crate::utils::changesets::objects(&a, "a", "p").is_empty());
    assert_eq!(crate::utils::changesets::objects(&a, "a", "q").len(), 1);
}

/// A round ending in a drop, with the system's entries and a `through` of the drop's version: the drop lands above the
/// watermark, then the entries are written and the watermark is the drop's version (7.8).
#[test]
fn test_a_round_ending_in_a_drop_writes_the_systems_entries_after_it() {
    use ultragraphdb_traits_store::VersionlessEntry;
    let a = crate::utils::changesets::changesets();
    crate::utils::changesets::write(&a, &["<a> <p> <b>", "<a> <q> <b>"]);
    let before = a.quads().lists().applied_ts();
    let write = a
        .execute(&crate::utils::changesets::changeset(vec![insert(
            "<c> <q> <d>",
        )]))
        .unwrap();
    let drop = a
        .prepare_drop_predicate(NS, crate::utils::changesets::ex("p").as_ref())
        .unwrap();
    let system = [VersionlessEntry::put(
        b"\xffsession".to_vec(),
        b"entry".to_vec(),
    )];
    a.apply_round_with(
        vec![(before + 2, write), (before + 6, drop)],
        &system,
        before + 6,
    )
    .unwrap();
    assert_eq!(a.quads().lists().applied_ts(), before + 6);
    assert!(crate::utils::changesets::objects(&a, "a", "p").is_empty());
    assert_eq!(crate::utils::changesets::objects(&a, "c", "q").len(), 1);
    assert_eq!(
        a.quads().store().get_versionless(b"\xffsession").unwrap(),
        Some(b"entry".to_vec())
    );
    // A round of the system's entries alone moves the watermark to `through`; one below it is refused.
    a.apply_round_with(Vec::new(), &system, before + 8).unwrap();
    assert_eq!(a.quads().lists().applied_ts(), before + 8);
    assert!(a.apply_round_with(Vec::new(), &system, before + 4).is_err());
}
