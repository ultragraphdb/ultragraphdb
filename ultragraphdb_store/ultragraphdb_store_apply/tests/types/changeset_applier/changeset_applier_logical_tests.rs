/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The logical record's derivation (write-ahead-log, "A round's record is logical"): an execution's statements in
//! their logical form, and replay deriving from them, into another store, every key and value the first apply wrote,
//! checked differentially over a workload of inserts, literals with indexes and counts, deletes, deletes by pattern and
//! edges with properties.

use std::collections::BTreeMap;

use oxrdf::{GraphName, NamedNode};
use ultragraphdb_core_keys::{Graph, Keyspace};
use ultragraphdb_core_mapping::Change;
use ultragraphdb_store_apply::{Executed, LogicalWrites};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_store::VersionlessEntry;

use crate::utils::changesets::{
    TestChangesets, changeset, changesets_over, declare, delete, delete_pattern, ex, insert,
    insert_edge, postings, sim_store,
};

/// The dictionary entries `executed` writes, the round's own beside its records.
fn dictionary(executed: &Executed) -> Vec<VersionlessEntry> {
    executed
        .writes()
        .versionless()
        .iter()
        .filter(|e| e.key().first() == Some(&Keyspace::Dictionary.byte()))
        .cloned()
        .collect()
}

/// Applies `changes` to `first` as a round of one at its next version, and to `replica` as replay does: the logical
/// writes the execution leaves, derived at the same version, its dictionary entries beside them.
fn both(
    first: &TestChangesets,
    replica: &TestChangesets,
    changes: Vec<ultragraphdb_core_mapping::Change>,
) -> TestResult {
    let executed = first.execute(&changeset(changes)).context("executed")?;
    let logical: LogicalWrites = executed.logical();
    let entries = dictionary(&executed);
    let version = first.quads().lists().next_version();
    let _applied = first
        .apply_round(vec![(version, executed)])
        .context("the first apply")?;
    let borrowed: Vec<&VersionlessEntry> = entries.iter().collect();
    let replayed = replica
        .prepare_logical(0, &logical, &borrowed)
        .context("the replay")?;
    let _replayed = replica
        .apply_round_with(vec![(version, replayed)], &entries, version)
        .context("the replayed apply")?;
    Ok(())
}

#[test]
fn test_an_executions_logical_writes_name_its_statements() -> TestResult {
    let a = changesets_over(sim_store())?;
    let executed = a
        .execute(&changeset(vec![
            insert("<a> <p> <b>")?,
            insert("<a> <name> \"A\"")?,
        ]))
        .context("executed")?;
    let logical = executed.logical();
    assert_eq!(logical.inserted().len(), 2);
    assert!(logical.removed().is_empty() && logical.records().is_empty());
    assert!(
        logical
            .inserted()
            .iter()
            .all(|q| q.graph() == Graph::Default && q.statement().is_none())
    );
    Ok(())
}

/// A record holds what the transaction changed: a parallel edge its own ID, whatever the object holds, and a deleted statement of
/// several its ID alone, the object staying. Replay adds and removes those IDs, so every key comes out as the first apply wrote it.
#[test]
fn test_a_record_logs_the_statement_ids_a_write_changed_alone() -> TestResult {
    let (first, replica) = (changesets_over(sim_store())?, changesets_over(sim_store())?);
    let edge = "<a> <knows> <b>";
    both(&first, &replica, vec![insert(edge)?])?;
    both(&first, &replica, vec![insert_edge(edge, &[])?])?;
    both(&first, &replica, vec![insert_edge(edge, &[])?])?;
    let held = postings(&first, "a", "knows")?;
    let ids = &held.first().context("the object")?.1;
    let middle = *ids.get(1).context("three statements")?;
    let parallel = first
        .execute(&changeset(vec![insert_edge(edge, &[])?]))
        .context("one more")?;
    let logged = parallel.logical();
    assert_eq!(logged.inserted().len(), 1, "{:?}", logged.inserted());
    assert!(
        logged
            .inserted()
            .iter()
            .all(|q| q.statement().is_some_and(|id| !ids.contains(&id))),
        "its own ID, none the object held"
    );
    first.release(&parallel);
    let reifier = NamedNode::new(first.quads().iri_of(0, middle)?.0)?;
    let delete_middle = Change::DeleteStatement {
        graph: GraphName::DefaultGraph,
        reifier,
    };
    let deleting = first
        .execute(&changeset(vec![delete_middle.clone()]))
        .context("the deletion")?;
    let removal = deleting.logical();
    assert!(removal.inserted().is_empty(), "{:?}", removal.inserted());
    assert_eq!(
        removal
            .removed()
            .iter()
            .map(|q| q.statement())
            .collect::<Vec<_>>(),
        vec![Some(middle)]
    );
    first.release(&deleting);
    both(&first, &replica, vec![delete_middle])?;
    let left = postings(&first, "a", "knows")?;
    assert_eq!(left.first().context("the object")?.1.len(), ids.len() - 1);
    assert_eq!(postings(&replica, "a", "knows")?, left);
    assert_eq!(
        first.quads().store().contents(),
        replica.quads().store().contents()
    );
    Ok(())
}

/// A transaction that wrote nothing gives its tenant no catalog, so it resolves no `rdf:type`, and replay of its empty record does
/// the same: a partition need not hold that ID to commit a read.
#[test]
fn test_an_execution_holding_no_write_resolves_no_catalog() -> TestResult {
    let a = changesets_over(sim_store())?;
    let read_ts = a.quads().lists().applied_ts();
    let executed = a.begin(0, read_ts).context("begun")?.finish()?;
    assert_eq!(executed.logical(), LogicalWrites::default());
    assert!(executed.terms().entries().is_empty(), "no term resolved");
    assert!(executed.writes().versionless().is_empty());
    a.release(&executed);
    let replayed = a
        .prepare_logical(0, &LogicalWrites::default(), &[])
        .context("the replay")?;
    assert!(replayed.terms().entries().is_empty(), "no term resolved");
    assert!(replayed.writes().versionless().is_empty());
    a.release(&replayed);
    Ok(())
}

#[test]
fn test_replay_derives_every_key_the_first_apply_wrote() -> TestResult {
    let first = changesets_over(sim_store())?;
    let replica = changesets_over(sim_store())?;
    // The catalog is set up alike in both, as the metadata group will hand it to every replica.
    for store in [&first, &replica] {
        declare(store, "name", "string @index(term) @count .")?;
    }
    both(
        &first,
        &replica,
        vec![insert("<a> <p> <b>")?, insert("<a> <name> \"Ann Lee\"")?],
    )?;
    both(
        &first,
        &replica,
        vec![insert("<b> <p> <c>")?, insert("<a> <name> \"Bo\"")?],
    )?;
    both(
        &first,
        &replica,
        vec![insert_edge("<a> <knows> <c>", &[("since", "\"2020\"")])?],
    )?;
    both(
        &first,
        &replica,
        vec![
            delete("<a> <name> \"Ann Lee\"")?,
            insert("<c> <name> \"Cy\"")?,
        ],
    )?;
    both(&first, &replica, vec![delete_pattern("a", "p")])?;
    both(&first, &replica, vec![delete("<a> <knows> <c>")?])?;
    let (a, b) = (
        first.quads().store().contents(),
        replica.quads().store().contents(),
    );
    assert_eq!(
        differences(&a, &b),
        Vec::<String>::new(),
        "every key and value, versions included"
    );
    assert_eq!(
        a.versionless(),
        b.versionless(),
        "the dictionary and the entries without versions"
    );
    assert_eq!(a, b);
    assert_eq!(
        first.quads().lists().applied_ts(),
        replica.quads().lists().applied_ts()
    );
    Ok(())
}

/// Two executions of one round both name `<new>`, the first creating it and the second taking it while pending: each
/// carries its entries, the round's dictionary holds them once, as the commit step gathers it, and replay derives what
/// the first apply wrote.
#[test]
fn test_two_executions_of_one_round_carrying_one_entry_replay() -> TestResult {
    let first = changesets_over(sim_store())?;
    let replica = changesets_over(sim_store())?;
    let one = first.execute(&changeset(vec![insert("<new> <p> <a>")?]))?;
    let two = first.execute(&changeset(vec![insert("<b> <p> <new>")?]))?;
    let mut round: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();
    let mut shared = 0usize;
    for e in dictionary(&one).iter().chain(&dictionary(&two)) {
        let value = e.value().context("an entry holds a value")?.to_vec();
        if let Some(earlier) = round.insert(e.key().to_vec(), value.clone()) {
            assert_eq!(earlier, value, "one entry, one value");
            shared += 1;
        }
    }
    assert!(shared >= 2, "both carry <new>'s entries, both directions");
    let logical = [one.logical(), two.logical()];
    let version = first.quads().lists().next_version();
    let _applied = first.apply_round(vec![(version, one), (version + 2, two)])?;
    let entries: Vec<VersionlessEntry> = round
        .into_iter()
        .map(|(k, v)| VersionlessEntry::put(k, v))
        .collect();
    let borrowed: Vec<&VersionlessEntry> = entries.iter().collect();
    let derived = [
        replica.prepare_logical(0, &logical[0], &borrowed)?,
        replica.prepare_logical(0, &logical[1], &borrowed)?,
    ];
    let [d1, d2] = derived;
    let _replayed = replica.apply_round_with(
        vec![(version, d1), (version + 2, d2)],
        &entries,
        version + 2,
    )?;
    let (a, b) = (
        first.quads().store().contents(),
        replica.quads().store().contents(),
    );
    assert_eq!(differences(&a, &b), Vec::<String>::new());
    assert_eq!(a, b);
    Ok(())
}

/// A write takes `<new>` while its creator's write is pending; the creator is refused and released, writing nothing,
/// and the taker carries `<new>`'s entries into its own commit, so the first store and the replica both name it.
#[test]
fn test_a_write_taking_a_refused_creators_id_commits_its_entries_and_replays() -> TestResult {
    let first = changesets_over(sim_store())?;
    let replica = changesets_over(sim_store())?;
    let refused = first.execute(&changeset(vec![insert("<new> <p> <a>")?]))?;
    let created = refused
        .terms()
        .written()
        .find(|(t, _)| *t == ex("new").as_str())
        .map(|(_, id)| id)
        .context("the creator created <new>")?;
    let taker = first.execute(&changeset(vec![insert("<b> <p> <new>")?]))?;
    first.release(&refused);
    assert_eq!(first.quads().id_of(0, ex("new").as_str())?, None);
    let logical = taker.logical();
    let entries = dictionary(&taker);
    let version = first.quads().lists().next_version();
    let _applied = first.apply_round(vec![(version, taker)])?;
    let borrowed: Vec<&VersionlessEntry> = entries.iter().collect();
    let derived = replica.prepare_logical(0, &logical, &borrowed)?;
    let _replayed = replica.apply_round_with(vec![(version, derived)], &entries, version)?;
    for a in [&first, &replica] {
        assert_eq!(a.quads().id_of(0, ex("new").as_str())?, Some(created));
        assert_eq!(
            a.quads().iri_of(0, created)?,
            (ex("new").as_str().to_string(), false)
        );
    }
    assert_eq!(
        first.quads().store().contents(),
        replica.quads().store().contents()
    );
    Ok(())
}

/// Whether the catalog `store` installed declares each predicate of `names`, and whether its rounds asked the metadata
/// group for the predicate's default entry.
fn declares(store: &TestChangesets, names: &[&str]) -> TestResult<Vec<[bool; 2]>> {
    let asked: Vec<String> = store
        .take_default_entries()
        .iter()
        .map(|e| e.predicate().to_string())
        .collect();
    names
        .iter()
        .map(|name| {
            let id = store
                .quads()
                .id_of(0, ex(name).as_str())?
                .context("the predicate's IRI has an ID")?;
            let installed = store.catalog(0)?.predicate(id).is_some();
            Ok([installed, asked.contains(&ex(name).as_str().to_string())])
        })
        .collect()
}

/// A changeset inserting a statement on an undeclared predicate and deleting it leaves no statement on it: neither the
/// first apply nor the replay writes the predicate's implicit default entry, and the two stores agree.
#[test]
fn test_a_predicate_the_net_effect_does_not_keep_gets_no_default_entry() -> TestResult {
    let first = changesets_over(sim_store())?;
    let replica = changesets_over(sim_store())?;
    both(
        &first,
        &replica,
        vec![insert("<s> <new> \"x\"")?, delete("<s> <new> \"x\"")?],
    )?;
    for store in [&first, &replica] {
        assert_eq!(
            declares(store, &["new"])?,
            vec![[false, false]],
            "no default entry"
        );
    }
    let (a, b) = (
        first.quads().store().contents(),
        replica.quads().store().contents(),
    );
    assert_eq!(differences(&a, &b), Vec::<String>::new());
    assert_eq!(a, b);
    Ok(())
}

/// A changeset keeping a statement on one undeclared predicate and undoing its statement on another: the first apply
/// and the replay both write the kept predicate's default entry and neither the other's.
#[test]
fn test_a_predicate_the_net_effect_keeps_gets_its_default_entry_on_both() -> TestResult {
    let first = changesets_over(sim_store())?;
    let replica = changesets_over(sim_store())?;
    both(
        &first,
        &replica,
        vec![
            insert("<s> <kept> \"x\"")?,
            insert("<s> <gone> \"y\"")?,
            delete("<s> <gone> \"y\"")?,
        ],
    )?;
    for store in [&first, &replica] {
        assert_eq!(
            declares(store, &["kept", "gone"])?,
            vec![[true, true], [false, false]],
            "the kept predicate's default entry alone"
        );
    }
    let (a, b) = (
        first.quads().store().contents(),
        replica.quads().store().contents(),
    );
    assert_eq!(differences(&a, &b), Vec::<String>::new());
    assert_eq!(a, b);
    Ok(())
}

/// The keys whose versions differ between `a` and `b`, each with both sides, for a failure to name.
fn differences(
    a: &ultragraphdb_store_sim::Contents,
    b: &ultragraphdb_store_sim::Contents,
) -> Vec<String> {
    let keys: std::collections::BTreeSet<&Vec<u8>> =
        a.keys().keys().chain(b.keys().keys()).collect();
    keys.into_iter()
        .filter(|k| a.versions(k) != b.versions(k))
        .map(|k| {
            format!(
                "{k:?}: first {:?} replica {:?}",
                a.versions(k),
                b.versions(k)
            )
        })
        .collect()
}

/// Replay's execution carries the logical writes it derived its keys from, statements and statement records alike: its logical
/// form is the record it replays.
#[test]
fn test_a_replayed_execution_s_logical_writes_are_those_it_derived_from() -> TestResult {
    let (first, replica) = (changesets_over(sim_store())?, changesets_over(sim_store())?);
    let executed = first
        .execute(&changeset(vec![
            insert("<a> <p> <b>")?,
            insert_edge("<a> <knows> <c>", &[("since", "\"2020\"")])?,
        ]))
        .context("executed")?;
    let logical = executed.logical();
    assert!(!logical.inserted().is_empty() && !logical.records().is_empty());
    let entries = dictionary(&executed);
    let borrowed: Vec<&VersionlessEntry> = entries.iter().collect();
    let replayed = replica
        .prepare_logical(0, &logical, &borrowed)
        .context("the replay")?;
    assert_eq!(replayed.logical(), logical);
    first.release(&executed);
    replica.release(&replayed);
    Ok(())
}

/// Replay of a record holding statements and no statement record gives a tenant without a catalog its initial one, as the
/// first apply did: the replica declares what the first declares, `rdf:type` among it.
#[test]
fn test_replay_of_statements_alone_gives_the_tenant_its_initial_catalog() -> TestResult {
    let (first, replica) = (changesets_over(sim_store())?, changesets_over(sim_store())?);
    both(&first, &replica, vec![insert("<a> <p> <b>")?])?;
    let declared = |store: &TestChangesets| -> TestResult<Vec<ultragraphdb_core_ids::SystemId>> {
        Ok(store.catalog(0)?.predicates().map(|(id, _)| id).collect())
    };
    let rdf_type = first
        .quads()
        .id_of(0, oxrdf::vocab::rdf::TYPE.as_str())?
        .context("rdf:type")?;
    assert!(declared(&first)?.contains(&rdf_type), "the initial catalog");
    assert_eq!(declared(&replica)?, declared(&first)?);
    Ok(())
}
