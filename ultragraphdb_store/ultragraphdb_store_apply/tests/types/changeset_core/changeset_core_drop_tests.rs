/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Drops at their edges: a predicate the store never named, a predicate declared without data, a predicate in several
//! graphs with its index keys and statement records, graphs the store does not hold, a namespace never written, a
//! drop during a build; and the dictionary every drop keeps.

use oxrdf::{BlankNode, GraphName, GraphNameRef};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{Graph, Keyspace, QuadKey};

use ultragraphdb_traits_store::Store;

use ultragraphdb_store_apply::{Applied, ChangesetError};

use crate::utils::changesets::{
    NS, TestChangesets, changesets, declaration, declare, ex, finish_builds, indexed, insert,
    insert_edge, keys_of, objects_in, predicate_list, stored_keys, term, term_token, uid, write,
    write_changes,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

fn names(a: &TestChangesets, predicate: SystemId) -> Vec<Vec<u8>> {
    stored_keys(a)
        .into_iter()
        .filter(|k| QuadKey::parse(k).is_ok_and(|(q, _)| q.predicate() == Some(predicate)))
        .collect()
}

#[test]
fn test_dropping_a_predicate_the_store_never_named_changes_nothing() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <knows> <b>"])?;
    let before = a.quads().store().contents();
    let latest = a.quads().lists().applied_ts();
    assert_eq!(a.drop_predicate(NS, ex("never").as_ref())?, latest);
    assert_eq!(a.quads().store().contents(), before);
    Ok(())
}

#[test]
fn test_dropping_a_predicate_without_data_removes_its_declaration() -> TestResult {
    let a = changesets()?;
    declare(&a, "name", "with `exact`")?;
    let p = uid(&a, "name")?;
    let _dropped_at: u64 = a.drop_predicate(NS, ex("name").as_ref())?;
    assert_eq!(a.catalog(NS)?.predicate(p), None);
    assert!(names(&a, p).is_empty());
    // The dictionary keeps its UID, so a second drop finds it and drops nothing.
    assert_eq!(uid(&a, "name")?, p);
    let _dropped_again_at: u64 = a.drop_predicate(NS, ex("name").as_ref())?;
    assert!(names(&a, p).is_empty());
    Ok(())
}

#[test]
fn test_dropping_a_predicate_removes_it_from_every_graph_and_keyspace() -> TestResult {
    let a = changesets()?;
    declare(&a, "friend", "with an exact count index and `term`")?;
    write_changes(
        &a,
        vec![
            insert("<a> <friend> <b>")?,
            insert("<a> <friend> \"best\" <g1>")?,
            insert("<a> <name> \"Ann\" <g1>")?,
            insert_edge("<c> <friend> <b> <g1>", &[("since", "2020")])?,
        ],
    )?;
    let friend = uid(&a, "friend")?;
    for keyspace in [
        Keyspace::Forward,
        Keyspace::Reverse,
        Keyspace::ValueIndex,
        Keyspace::CountIndex,
        Keyspace::Statistics,
    ] {
        assert!(
            !keys_of(&a, keyspace, "friend")?.is_empty(),
            "{keyspace:?} before"
        );
    }
    let versionless = a.quads().store().contents().versionless().clone();
    let _dropped_at: u64 = a.drop_predicate(NS, ex("friend").as_ref())?;
    assert!(names(&a, friend).is_empty(), "{:?}", names(&a, friend));
    assert!(
        !stored_keys(&a)
            .iter()
            .any(|k| matches!(Keyspace::from_byte(k[0]), Some((Keyspace::Statements, _)))),
        "the record of <c> <friend> <b> in <g1> is gone"
    );
    assert!(predicate_list(&a, "a", false)?.is_empty());
    assert!(predicate_list(&a, "b", true)?.is_empty());
    assert_eq!(objects_in(&a, "g1", "a", "name")?, vec![term("\"Ann\"")?]);
    assert_eq!(a.catalog(NS)?.predicate(friend), None);
    // The dictionary is kept: every versionless entry before the drop is still there.
    let after = a.quads().store().contents().versionless().clone();
    assert!(versionless.iter().all(|(k, v)| after.get(k) == Some(v)));
    Ok(())
}

#[test]
fn test_dropping_a_graph_the_store_does_not_hold_changes_nothing() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <knows> <b> <g1>"])?;
    let before = a.quads().store().contents();
    let latest = a.quads().lists().applied_ts();
    assert_eq!(
        a.drop_graph(NS, GraphName::from(ex("never")).as_ref())?,
        latest
    );
    let blank = BlankNode::new_unchecked("g");
    assert_eq!(
        a.drop_graph(NS, GraphNameRef::BlankNode(blank.as_ref()))?,
        latest
    );
    assert_eq!(a.quads().store().contents(), before);
    Ok(())
}

#[test]
fn test_dropping_a_graph_removes_its_index_keys_and_keeps_the_other_graphs() -> TestResult {
    let a = changesets()?;
    declare(&a, "name", "with `term`")?;
    declare(&a, "friend", "with an exact count index")?;
    write(
        &a,
        &[
            "<a> <name> \"big apple\" <g1>",
            "<a> <friend> <b> <g1>",
            "<a> <name> \"big apple\"",
            "<a> <friend> <b>",
        ],
    )?;
    let g1 = Graph::Named(uid(&a, "g1")?);
    let _dropped_at: u64 = a.drop_graph(NS, GraphName::from(ex("g1")).as_ref())?;
    let left: Vec<Vec<u8>> = stored_keys(&a)
        .into_iter()
        .filter(|k| QuadKey::parse(k).is_ok_and(|(q, _)| q.graph() == Some(g1)))
        .collect();
    assert!(left.is_empty(), "{left:?}");
    assert_eq!(
        indexed(&a, "name", &term_token("big"))?,
        vec![uid(&a, "a")?]
    );
    assert_eq!(objects_in(&a, "", "a", "friend")?, vec![term("<b>")?]);
    Ok(())
}

#[test]
fn test_dropping_the_default_graph_keeps_the_named_ones() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <knows> <b>", "<a> <knows> <c> <g1>"])?;
    let _dropped_at: u64 = a.drop_graph(NS, GraphNameRef::DefaultGraph)?;
    assert!(objects_in(&a, "", "a", "knows")?.is_empty());
    assert_eq!(objects_in(&a, "g1", "a", "knows")?, vec![term("<c>")?]);
    Ok(())
}

#[test]
fn test_dropping_a_namespace_never_written_writes_no_catalog() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <knows> <b>"])?;
    let before = a.quads().store().contents();
    let _dropped_at: u64 = a.drop_namespace(5)?;
    assert_eq!(a.quads().store().contents().keys(), before.keys());
    assert!(a.catalog(5)?.predicates().next().is_none());
    Ok(())
}

#[test]
fn test_dropping_the_predicate_being_built_cancels_its_build() -> TestResult {
    let a = changesets()?;
    declare(&a, "name", "with `term`")?;
    write(&a, &["<a> <name> \"big apple\""])?;
    declare(&a, "name", "with `exact`")?;
    assert!(a.rebuilding());
    let p = uid(&a, "name")?;
    let _dropped_at: u64 = a.drop_predicate(NS, ex("name").as_ref())?;
    assert!(!a.rebuilding());
    finish_builds(&a);
    assert!(names(&a, p).is_empty(), "{:?}", names(&a, p));
    assert_eq!(a.catalog(NS)?.predicate(p), None);
    assert!(a.quads().store().rebuild_records()?.is_empty());
    Ok(())
}

#[test]
fn test_dropping_the_data_during_a_build_declares_the_new_declaration() -> TestResult {
    let a = changesets()?;
    declare(&a, "name", "with `term`")?;
    write(&a, &["<a> <name> \"big apple\""])?;
    declare(&a, "name", "with `exact`")?;
    let _dropped_at: u64 = a.drop_data(NS)?;
    assert!(!a.rebuilding());
    finish_builds(&a);
    let p = uid(&a, "name")?;
    assert_eq!(
        a.catalog(NS)?.predicate(p),
        Some(&declaration("name", "with `exact`")?)
    );
    assert!(a.quads().store().rebuild_records()?.is_empty());
    // Writes after the drop are indexed under the new declaration alone.
    write(&a, &["<b> <name> \"pear\""])?;
    assert!(indexed(&a, "name", &term_token("pear"))?.is_empty());
    assert_eq!(keys_of(&a, Keyspace::ValueIndex, "name")?.len(), 1);
    Ok(())
}

#[test]
fn test_each_drop_returns_its_own_timestamp() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <knows> <b>", "<a> <knows> <c> <g1>"])?;
    let drops: [&dyn Fn() -> Result<u64, ChangesetError>; 5] = [
        &|| a.drop_predicate(NS, ex("knows").as_ref()),
        &|| a.drop_graph(NS, GraphName::from(ex("g1")).as_ref()),
        &|| a.drop_data(NS),
        &|| a.drop_namespace(NS),
        &|| a.drop_all(),
    ];
    for (i, drop) in drops.iter().enumerate() {
        write(&a, &["<a> <knows> <b>", "<a> <knows> <c> <g1>"])?;
        let before = a.quads().lists().applied_ts();
        let ts = drop()?;
        assert!(ts > before, "drop {i}: {ts} after {before}");
        assert_eq!(ts, a.quads().lists().applied_ts(), "drop {i}");
    }
    Ok(())
}

#[test]
fn test_dropping_another_namespace_keeps_the_build() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <name> \"pear\""])?;
    let _applied: Applied =
        a.apply(&ultragraphdb_core_mapping::Changeset::new(3).with(insert("<a> <name> \"fig\"")?))?;
    declare(&a, "name", "with `exact`")?;
    let _dropped_at: u64 = a.drop_namespace(3)?;
    let _data_dropped_at: u64 = a.drop_data(3)?;
    assert!(a.rebuilding());
    finish_builds(&a);
    assert_eq!(
        indexed(&a, "name", &crate::utils::changesets::exact_token("pear"))?,
        vec![uid(&a, "a")?]
    );
    Ok(())
}

#[test]
fn test_dropping_a_predicate_rewrites_a_split_predicate_list_whole() -> TestResult {
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
    let a = TestChangesets::new(
        lists,
        crate::utils::changesets::blocks(),
        ultragraphdb_core_mapping::RandomSource::new(1),
        executor,
        1,
    )?;
    let changes = (0..2_000)
        .map(|i| insert(&format!("<a> <p{i}> <o>")))
        .collect::<TestResult<_>>()?;
    write_changes(&a, changes)?;
    let key = QuadKey::SubjectPredicates {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        subject: uid(&a, "a")?,
    }
    .to_bytes();
    let _rollup_key: bool = a.quads().lists().rollup_key(&key)?;
    let latest = a.quads().lists().applied_ts();
    // The rollup is written one above the commit it folds, past the watermark: a read of everything reaches it.
    assert!(
        !a.quads()
            .lists()
            .read(&key, u64::MAX)?
            .plist()
            .splits()
            .is_empty(),
        "the list is split"
    );
    let _dropped_at: u64 = a.drop_predicate(NS, ex("p0").as_ref())?;
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
    let held = predicate_list(&a, "a", false)?;
    assert_eq!(held.len(), 1_999);
    assert!(!held.contains(&uid(&a, "p0")?));
    Ok(())
}

/// A drop prepared for the commit step writes the predicate's key ranges, its catalog entry's among them, and nothing
/// else; applied in a round at its version, it drops the predicate, and a predicate the store does not know is no drop
/// (7.7).
#[test]
fn test_a_drop_prepared_for_a_round_writes_its_ranges_and_drops_at_its_version() -> TestResult {
    let a = changesets()?;
    crate::utils::changesets::write(&a, &["<a> <p> <b>", "<a> <q> <b>"])?;
    let unknown = a.prepare_drop(
        NS,
        ultragraphdb_store_apply::DropRequest::Predicate(ex("none").as_ref()),
    )?;
    assert!(!unknown.is_drop());
    assert!(unknown.drop_ranges().is_empty());
    let p = uid(&a, "p")?;
    let drop = a.prepare_drop(
        NS,
        ultragraphdb_store_apply::DropRequest::Predicate(ex("p").as_ref()),
    )?;
    assert!(drop.is_drop());
    assert!(!drop.starts_build());
    let catalog = ultragraphdb_core_keys::Keyspace::Catalog.byte();
    assert!(
        drop.drop_ranges()
            .iter()
            .all(|(start, _)| start.first() != Some(&catalog)),
        "no range of the catalog keyspace, which no partition writes"
    );
    assert!(!drop.drop_ranges().is_empty(), "the ranges of {p:?}'s keys");
    assert!(drop.drop_ranges().iter().all(|(s, e)| s < e));
    assert!(drop.writes().deltas().is_empty());
    let version = a.quads().lists().applied_ts() + 2;
    let _applied: Vec<Applied> = a.apply_round(vec![(version, drop)])?;
    assert_eq!(a.quads().lists().applied_ts(), version);
    assert!(crate::utils::changesets::objects(&a, "a", "p")?.is_empty());
    assert_eq!(crate::utils::changesets::objects(&a, "a", "q")?.len(), 1);
    Ok(())
}

/// A round ending in a drop, with the system's entries and a `through` of the drop's version: the drop lands above the
/// watermark, then the entries are written and the watermark is the drop's version (7.8).
#[test]
fn test_a_round_ending_in_a_drop_writes_the_systems_entries_after_it() -> TestResult {
    use ultragraphdb_traits_store::VersionlessEntry;
    let a = changesets()?;
    crate::utils::changesets::write(&a, &["<a> <p> <b>", "<a> <q> <b>"])?;
    let before = a.quads().lists().applied_ts();
    let write = a.execute(&crate::utils::changesets::changeset(vec![insert(
        "<c> <q> <d>",
    )?]))?;
    let drop = a.prepare_drop(
        NS,
        ultragraphdb_store_apply::DropRequest::Predicate(ex("p").as_ref()),
    )?;
    let system = [VersionlessEntry::put(
        b"\xffsession".to_vec(),
        b"entry".to_vec(),
    )];
    let _applied: Vec<Applied> = a.apply_round_with(
        vec![(before + 2, write), (before + 6, drop)],
        &system,
        before + 6,
    )?;
    assert_eq!(a.quads().lists().applied_ts(), before + 6);
    assert!(crate::utils::changesets::objects(&a, "a", "p")?.is_empty());
    assert_eq!(crate::utils::changesets::objects(&a, "c", "q")?.len(), 1);
    assert_eq!(
        a.quads().store().get_versionless(b"\xffsession")?,
        Some(b"entry".to_vec())
    );
    // A round of the system's entries alone moves the watermark to `through`; one below it is refused.
    let _system_applied: Vec<Applied> = a.apply_round_with(Vec::new(), &system, before + 8)?;
    assert_eq!(a.quads().lists().applied_ts(), before + 8);
    let refused = a
        .apply_round_with(Vec::new(), &system, before + 4)
        .err()
        .context("the error of a.apply_round_with(..)")?;
    // `through` at before + 4 is below the watermark, before + 8.
    assert_eq!(
        refused.0,
        ultragraphdb_store_apply::ChangesetErrorEnum::Posting(
            ultragraphdb_store_posting::PostingError(
                ultragraphdb_store_posting::PostingErrorEnum::StaleVersion {
                    version: before + 4,
                    after: before + 8
                }
            )
        )
    );
    assert_eq!(a.quads().lists().applied_ts(), before + 8);
    Ok(())
}

/// The store a drop of each kind is tried on: statements in the default graph, in `g1` and in another namespace, and a
/// declaration, so every kind leaves something and takes something.
fn dropping_store() -> TestResult<TestChangesets> {
    let a = changesets()?;
    declare(&a, "name", "a property with `term`")?;
    write(
        &a,
        &[
            "<a> <name> \"big apple\"",
            "<a> <friend> <b>",
            "<a> <name> \"pear\" <g1>",
            "<a> <friend> <c> <g1>",
            // `rdf:type` has a UID, so a namespace dropped writes its initial catalog again.
            "<a> rdf:type <Fruit>",
        ],
    )?;
    let _applied: Applied =
        a.apply(&ultragraphdb_core_mapping::Changeset::new(7).with(insert("<x> <friend> <y>")?))?;
    Ok(a)
}

/// What a drop of `request` decided in a round leaves: the store's keys, after the drop prepared for the commit step
/// and applied at the round's next version.
fn dropped_in_a_round(
    request: ultragraphdb_store_apply::DropRequest<'_>,
) -> TestResult<Vec<Vec<u8>>> {
    let a = dropping_store()?;
    let drop = a.prepare_drop(NS, request)?;
    assert!(drop.is_drop());
    let version = a.quads().lists().applied_ts() + 2;
    let _applied: Vec<Applied> = a.apply_round(vec![(version, drop)])?;
    assert_eq!(a.quads().lists().applied_ts(), version);
    Ok(stored_keys(&a))
}

/// A drop's name, its request decided in a round, and its immediate drop, the oracle.
type DropCase<'a> = (
    &'a str,
    ultragraphdb_store_apply::DropRequest<'a>,
    fn(&TestChangesets) -> Result<u64, ChangesetError>,
);

/// Each kind of drop decided in a round leaves the keys its immediate drop leaves: a graph, a namespace's data, the
/// namespace, every namespace (`PROTOCOL.md` §6.5, §12.2 operations 4 to 7).
#[test]
fn test_each_kind_of_drop_decided_in_a_round_drops_what_its_immediate_drop_does() -> TestResult {
    use ultragraphdb_store_apply::DropRequest;
    let g1 = ex("g1");
    let cases: [DropCase<'_>; 5] = [
        (
            "graph",
            DropRequest::Graph(GraphNameRef::NamedNode(g1.as_ref())),
            |a| a.drop_graph(NS, GraphName::from(ex("g1")).as_ref()),
        ),
        (
            "default graph",
            DropRequest::Graph(GraphNameRef::DefaultGraph),
            |a| a.drop_graph(NS, GraphNameRef::DefaultGraph),
        ),
        ("data", DropRequest::Data, |a| a.drop_data(NS)),
        ("namespace", DropRequest::Namespace, |a| {
            a.drop_namespace(NS)
        }),
        ("all", DropRequest::All, |a| a.drop_all()),
    ];
    for (name, request, immediate) in cases {
        let oracle = dropping_store()?;
        let _dropped_at: u64 = immediate(&oracle)?;
        let before = dropping_store()?;
        let in_a_round = dropped_in_a_round(request)?;
        assert_eq!(in_a_round, stored_keys(&oracle), "{name}");
        assert_ne!(
            in_a_round,
            stored_keys(&before),
            "{name}: something dropped"
        );
    }
    Ok(())
}

/// A drop prepared for a round names what it drops and writes the key ranges of it: a graph's in every quad keyspace,
/// a namespace's data without its catalog, a namespace with it, every namespace's every keyspace.
#[test]
fn test_a_drop_prepared_for_a_round_names_its_target_and_its_ranges() -> TestResult {
    use ultragraphdb_core_keys::key_prefix;
    use ultragraphdb_store_apply::{DropRequest, DropTarget};
    let a = dropping_store()?;
    let g1 = Graph::Named(uid(&a, "g1")?);
    let graph = a.prepare_drop(NS, DropRequest::Graph(GraphName::from(ex("g1")).as_ref()))?;
    assert_eq!(graph.dropped(), Some(DropTarget::Graph(g1)));
    let forward = key_prefix(Keyspace::Forward, false, NS, Some(g1), None);
    assert!(
        graph
            .drop_ranges()
            .iter()
            .any(|(start, _)| start == &forward)
    );
    assert!(graph.drop_ranges().iter().all(|(s, e)| s < e));
    let data = a.prepare_drop(NS, DropRequest::Data)?;
    assert_eq!(data.dropped(), Some(DropTarget::Data));
    let catalog = key_prefix(Keyspace::Catalog, false, NS, None, None);
    assert!(
        !data
            .drop_ranges()
            .iter()
            .any(|(start, _)| start == &catalog),
        "the catalog is kept"
    );
    let namespace = a.prepare_drop(NS, DropRequest::Namespace)?;
    assert_eq!(namespace.dropped(), Some(DropTarget::Namespace));
    // No partition holds a catalog key, so no drop ranges over the catalog keyspace: a namespace's drop covers its
    // quad keyspaces, as its data's does.
    assert!(
        !namespace
            .drop_ranges()
            .iter()
            .any(|(start, _)| start == &catalog)
    );
    assert_eq!(namespace.drop_ranges(), data.drop_ranges());
    let all = a.prepare_drop(NS, DropRequest::All)?;
    assert_eq!(all.dropped(), Some(DropTarget::All));
    assert!(
        !all.drop_ranges()
            .iter()
            .any(|(start, _)| start.first() == Some(&Keyspace::Catalog.byte()))
    );
    assert!(!all.drop_ranges().is_empty());
    for drop in [&graph, &data, &namespace, &all] {
        assert!(drop.writes().deltas().is_empty());
    }
    Ok(())
}

/// A graph the store does not know, and a blank-node graph, are dropped as nothing: no drop.
#[test]
fn test_a_drop_of_a_graph_the_store_does_not_hold_is_no_drop() -> TestResult {
    use ultragraphdb_store_apply::DropRequest;
    let a = dropping_store()?;
    let never = a.prepare_drop(
        NS,
        DropRequest::Graph(GraphName::from(ex("never")).as_ref()),
    )?;
    let blank = BlankNode::new_unchecked("g");
    let blank = a.prepare_drop(
        NS,
        DropRequest::Graph(GraphNameRef::BlankNode(blank.as_ref())),
    )?;
    for drop in [never, blank] {
        assert!(!drop.is_drop());
        assert_eq!(drop.dropped(), None);
        assert!(drop.drop_ranges().is_empty());
    }
    Ok(())
}

/// A drop replayed from its record drops what the drop decided in the round did.
#[test]
fn test_each_kind_of_drop_replayed_drops_what_it_dropped() -> TestResult {
    use ultragraphdb_store_apply::{DropRequest, DropTarget};
    let g1 = ex("g1");
    let g1_uid = Graph::Named(uid(&dropping_store()?, "g1")?);
    for (request, target) in [
        (
            DropRequest::Graph(GraphNameRef::NamedNode(g1.as_ref())),
            DropTarget::Graph(g1_uid),
        ),
        (
            DropRequest::Graph(GraphNameRef::DefaultGraph),
            DropTarget::Graph(ultragraphdb_core_keys::DEFAULT_GRAPH),
        ),
        (DropRequest::Data, DropTarget::Data),
        (DropRequest::Namespace, DropTarget::Namespace),
        (DropRequest::All, DropTarget::All),
    ] {
        let decided = dropped_in_a_round(request)?;
        let a = dropping_store()?;
        let replayed = a.prepare_replayed_drop(NS, target)?;
        assert_eq!(replayed.dropped(), Some(target));
        let version = a.quads().lists().applied_ts() + 2;
        let _applied: Vec<Applied> = a.apply_round(vec![(version, replayed)])?;
        assert_eq!(stored_keys(&a), decided, "{target:?}");
    }
    Ok(())
}

/// A graph whose lists split is dropped whole in a round: no key of the graph remains, its lists' split parts with them.
#[test]
fn test_a_graph_dropped_in_a_round_takes_its_split_lists_parts() -> TestResult {
    use ultragraphdb_store_apply::DropRequest;
    use ultragraphdb_store_posting::{Lists, ListsConfig};
    use ultragraphdb_store_sim::{Seed, SimListCache, SteppedExecutor, Stream};
    use ultragraphdb_traits_store::PersistMode;
    // A list size of 256 bytes, so a subject holding 2,000 predicates in `g1` splits.
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    let executor = SteppedExecutor::new(Seed::new(1).stream(Stream::Executor));
    let lists = Lists::new(
        crate::utils::changesets::sim_store(),
        cache,
        ListsConfig::new(PersistMode::Buffer, 256),
    );
    let a = TestChangesets::new(
        lists,
        crate::utils::changesets::blocks(),
        ultragraphdb_core_mapping::RandomSource::new(1),
        executor,
        1,
    )?;
    let changes = (0..2_000)
        .map(|i| insert(&format!("<a> <p{i}> <o> <g1>")))
        .collect::<TestResult<_>>()?;
    write_changes(&a, changes)?;
    let g1 = Graph::Named(uid(&a, "g1")?);
    let key = QuadKey::SubjectPredicates {
        namespace: NS,
        graph: g1,
        subject: uid(&a, "a")?,
    }
    .to_bytes();
    let _rollup_key: bool = a.quads().lists().rollup_key(&key)?;
    let in_g1 = |store: &TestChangesets| -> usize {
        stored_keys(store)
            .into_iter()
            .filter(|k| QuadKey::parse(k).is_ok_and(|(q, _)| q.graph() == Some(g1)))
            .count()
    };
    let mut parts = key.clone();
    parts[0] |= ultragraphdb_core_keys::SPLIT_FLAG;
    assert!(
        stored_keys(&a).iter().any(|k| k.starts_with(&parts)),
        "the list is split"
    );
    let drop = a.prepare_drop(NS, DropRequest::Graph(GraphName::from(ex("g1")).as_ref()))?;
    let version = a.quads().lists().applied_ts() + 2;
    let _applied: Vec<Applied> = a.apply_round(vec![(version, drop)])?;
    assert_eq!(in_g1(&a), 0);
    assert!(
        !stored_keys(&a).iter().any(|k| k.starts_with(&parts)),
        "its split parts too"
    );
    Ok(())
}

/// A namespace dropped, at once or in a round, writes its initial catalog again: `rdf:type` declared as a new
/// namespace's catalog declares it, and the predicates declared before the drop are gone.
#[test]
fn test_a_namespace_dropped_keeps_its_initial_catalog() -> TestResult {
    use ultragraphdb_core_catalog::Catalog;
    use ultragraphdb_store_apply::DropRequest;
    let rdf_type = |a: &TestChangesets| -> TestResult<SystemId> {
        a.quads()
            .id_of(NS, oxrdf::vocab::rdf::TYPE.as_str())?
            .context("rdf:type has a UID")
    };
    let initial = |a: &TestChangesets| -> TestResult<_> {
        Ok(Catalog::initial(NS, rdf_type(a)?)
            .predicate(rdf_type(a)?)
            .cloned())
    };
    let at_once = dropping_store()?;
    let _dropped_at: u64 = at_once.drop_namespace(NS)?;
    let in_a_round = dropping_store()?;
    let drop = in_a_round.prepare_drop(NS, DropRequest::Namespace)?;
    let version = in_a_round.quads().lists().applied_ts() + 2;
    let _applied: Vec<Applied> = in_a_round.apply_round(vec![(version, drop)])?;
    for a in [&at_once, &in_a_round] {
        let catalog = a.catalog(NS)?;
        assert!(
            initial(a)?.is_some(),
            "the initial catalog declares rdf:type"
        );
        assert_eq!(catalog.predicate(rdf_type(a)?).cloned(), initial(a)?);
        assert_eq!(
            catalog.predicate(uid(a, "name")?),
            None,
            "the declaration of `name` is gone"
        );
    }
    Ok(())
}

/// The keys of `keyspace` the store holds.
fn held_in(a: &TestChangesets, keyspace: Keyspace) -> Vec<Vec<u8>> {
    stored_keys(a)
        .into_iter()
        .filter(|k| k.first() == Some(&keyspace.byte()))
        .collect()
}

/// A store holding a referrer set, of `<hub>`, and a mark, written by one transaction.
fn registered() -> TestResult<TestChangesets> {
    let a = changesets()?;
    let entity = SystemId::new(
        ultragraphdb_core_ids::TypePrefix::RESOURCE,
        crate::utils::partitions::bucket_of("o")?,
        ultragraphdb_core_ids::Sequence::new(5)?,
    );
    let mut execution = a.begin(NS, a.quads().lists().applied_ts())?;
    execution.mark(
        entity,
        ultragraphdb_core_ids::BucketGroup::new(7)?,
        std::num::NonZeroU64::MIN,
    )?;
    execution.apply(&crate::utils::changesets::changeset(vec![insert(
        "<s> <knows> <hub>",
    )?]))?;
    let executed = execution.finish()?;
    let at = a.quads().lists().next_version();
    let _applied: Vec<Applied> = a.apply_round(vec![(at, executed)])?;
    assert_eq!(held_in(&a, Keyspace::Referrers).len(), 1);
    assert_eq!(held_in(&a, Keyspace::Marks).len(), 1);
    Ok(a)
}

#[test]
fn test_dropping_a_namespace_s_data_or_everything_drops_its_marks_and_referrer_sets() -> TestResult
{
    let data = registered()?;
    let _data_dropped_at: u64 = data.drop_data(NS)?;
    let everything = registered()?;
    let _all_dropped_at: u64 = everything.drop_all()?;
    let namespace = registered()?;
    let _namespace_dropped_at: u64 = namespace.drop_namespace(NS)?;
    for a in [&data, &everything, &namespace] {
        assert!(held_in(a, Keyspace::Referrers).is_empty());
        assert!(held_in(a, Keyspace::Marks).is_empty());
    }
    Ok(())
}

#[test]
fn test_dropping_a_graph_keeps_the_marks_and_referrer_sets_which_name_none() -> TestResult {
    let a = registered()?;
    let _dropped_at: u64 = a.drop_graph(NS, GraphNameRef::DefaultGraph)?;
    assert_eq!(held_in(&a, Keyspace::Referrers).len(), 1);
    assert_eq!(held_in(&a, Keyspace::Marks).len(), 1);
    Ok(())
}
