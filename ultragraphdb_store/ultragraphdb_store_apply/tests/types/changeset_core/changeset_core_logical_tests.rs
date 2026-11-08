/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Replay of logical writes into a replica: a term the replica's dictionary and the round's entries both lack is resolved
//! by the replay itself, which writes its entries beside those it inherited.

use oxrdf::vocab::rdf;
use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_store_apply::Executed;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_store::VersionlessEntry;

use crate::utils::changesets::{TENANT, TestChangesets, changeset, changesets, insert};

/// The dictionary entries `executed` writes.
fn dictionary(executed: &Executed) -> Vec<VersionlessEntry> {
    executed
        .writes()
        .versionless()
        .iter()
        .filter(|e| e.key().first() == Some(&Keyspace::Dictionary.byte()))
        .cloned()
        .collect()
}

/// Whether `entry` names the ID `id`, as its key or its value.
fn names(entry: &VersionlessEntry, id: &[u8]) -> bool {
    let holds = |bytes: &[u8]| bytes.windows(id.len()).any(|w| w == id);
    holds(entry.key()) || entry.value().is_some_and(holds)
}

#[test]
fn test_a_replay_resolves_a_term_its_inherited_entries_lack_and_writes_its_entries() -> TestResult {
    let (first, replica): (TestChangesets, TestChangesets) = (changesets()?, changesets()?);
    let executed = first.execute(&changeset(vec![insert("<a> <knows> <b>")?]))?;
    let logical = executed.logical();
    let entries = dictionary(&executed);
    let version = first.quads().lists().next_version();
    let _applied = first.apply_round(vec![(version, executed)])?;
    let rdf_type = first
        .quads()
        .id_of(TENANT, rdf::TYPE.as_str())?
        .context("the first apply resolves rdf:type")?
        .to_be_bytes();
    // The round's entries without those naming rdf:type: the replica, whose tenant holds no catalog, resolves it itself.
    let inherited: Vec<&VersionlessEntry> =
        entries.iter().filter(|e| !names(e, &rdf_type)).collect();
    assert!(
        inherited.len() < entries.len(),
        "rdf:type's entries left out"
    );
    let replayed = replica.prepare_logical(TENANT, &logical, &inherited)?;
    let own: Vec<VersionlessEntry> = dictionary(&replayed)
        .into_iter()
        .filter(|e| !inherited.contains(&e))
        .collect();
    assert!(!own.is_empty(), "the replay's own entries");
    let _replayed = replica.apply_round(vec![(version, replayed)])?;
    assert!(
        replica.quads().id_of(TENANT, rdf::TYPE.as_str())?.is_some(),
        "the replica holds rdf:type"
    );
    Ok(())
}
