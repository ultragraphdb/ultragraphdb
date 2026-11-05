/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The steps of quad-storage's statement IDs on both postings, and of its regions: a parallel edge, replay deriving what the first
//! apply wrote, and two tenants sharing a bucket run.

use cucumber::{then, when};
use ultragraphdb_core_ids::{SystemId, Tenant};
use ultragraphdb_core_keys::{DEFAULT_GRAPH, Keyspace, QuadKey};
use ultragraphdb_core_mapping::Changeset;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_apply::Applied;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_traits_store::{Scan, Store, StoreSnapshot, VersionlessEntry};

use crate::Steps;
use crate::changeset_steps::cs;
use crate::utils::changesets::{
    TENANT, TestChangesets, apply, changeset, changesets, ex, insert, insert_edge, postings,
    reverse_postings, uid,
};

#[when(
    "`<a> <knows> <b>` is held without a statement ID, and a write inserts a parallel edge of it"
)]
fn parallel_edge(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let _held: Applied =
        apply(&a, vec![insert("<a> <knows> <b>")?]).context("the statement is written")?;
    let result = apply(&a, vec![insert_edge("<a> <knows> <b>", &[])?]);
    w.results.push(result);
    Ok(())
}

#[then(
    "the object is held under two statement IDs, one for each statement, in ascending order, in the forward list of `knows` for `a` and the reverse list of `knows` for `b` alike, and neither list holds `<b>` without an ID"
)]
fn two_ids_on_both_lists(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let refused = w.results.last().context("the write")?.as_ref().err();
    assert_eq!(refused, None);
    let forward = postings(&a, "a", "knows")?;
    let [(object, ids)] = forward.as_slice() else {
        return Err(TestError::new("one object"));
    };
    assert_eq!(*object, EncodedTerm::id(uid(&a, "b")?));
    let [first, second] = ids.as_slice() else {
        return Err(TestError::new("two statement IDs"));
    };
    assert!(first < second, "ascending");
    assert_eq!(
        reverse_postings(&a, "knows", "b")?,
        vec![(EncodedTerm::id(uid(&a, "a")?), ids.clone())]
    );
    Ok(())
}

/// A list's objects, each with its statement IDs.
type Postings = Vec<(EncodedTerm, Vec<SystemId>)>;

/// What `a` holds of `<a> <knows> ?` and `? <knows> <b>` and `? <knows> <c>`, each posting with its statement IDs.
fn both_postings(a: &TestChangesets) -> TestResult<Vec<Postings>> {
    Ok(vec![
        postings(a, "a", "knows")?,
        reverse_postings(a, "knows", "b")?,
        reverse_postings(a, "knows", "c")?,
    ])
}

#[when(
    "a round holding a statement's first property and a parallel edge is applied, the store is discarded, and the round's record is replayed into an empty store"
)]
fn replayed_into_an_empty_store(w: &mut Steps) -> TestResult {
    let first = changesets()?;
    let executed = first
        .execute(&changeset(vec![
            insert_edge("<a> <knows> <b>", &[("since", "2020")])?,
            insert("<a> <knows> <c>")?,
            insert_edge("<a> <knows> <c>", &[])?,
        ]))
        .context("the round executes")?;
    let logical = executed.logical();
    let entries: Vec<VersionlessEntry> = executed
        .writes()
        .versionless()
        .iter()
        .filter(|e| e.key().first() == Some(&Keyspace::Dictionary.byte()))
        .cloned()
        .collect();
    let version = first.quads().lists().next_version();
    let _first: Vec<Applied> = first.apply_round(vec![(version, executed)])?;
    let held = both_postings(&first)?;
    let [forward, _, _] = held.as_slice() else {
        return Err(TestError::new("three lists"));
    };
    assert!(
        forward.iter().all(|(_, ids)| !ids.is_empty()),
        "{forward:?}"
    );
    assert!(forward.iter().any(|(_, ids)| ids.len() == 2), "{forward:?}");
    let replica = changesets()?;
    let borrowed: Vec<&VersionlessEntry> = entries.iter().collect();
    let derived = replica.prepare_logical(TENANT, &logical, &borrowed)?;
    let _replayed: Vec<Applied> =
        replica.apply_round_with(vec![(version, derived)], &entries, version)?;
    w.flags.push(both_postings(&replica)? == held);
    Ok(())
}

#[then(
    "the forward and reverse postings hold the same statement IDs they held after the first apply"
)]
fn replay_holds_the_same(w: &mut Steps) -> TestResult {
    assert_eq!(w.flags.last(), Some(&true));
    Ok(())
}

/// The first tenant above 7 sharing 7's bucket run, the scenario's two tenants: which tenants share a run follows from their
/// hash, and 7 and 8 name any such pair.
fn run_sharer() -> TestResult<u64> {
    let run = Tenant::new(7).bucket_bits();
    (8..=u64::MAX)
        .find(|t| Tenant::new(*t).bucket_bits() == run)
        .context("a tenant sharing the run")
}

/// `<a> <p> <b>` in `tenant`.
fn written_in(a: &TestChangesets, tenant: u64) -> TestResult {
    let _applied: Applied = a
        .apply(&Changeset::new(tenant).with(insert("<a> <p> <b>")?))
        .context("the statement is written")?;
    Ok(())
}

#[when(
    "tenants 7 and 8 share a bucket run, and each writes `<a> <p> <b>`, whose subject takes one bucket in both"
)]
fn two_tenants_write(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let other = run_sharer()?;
    written_in(&a, 7)?;
    written_in(&a, other)?;
    let subject = ex("a");
    let ids = (
        a.quads()
            .id_of(7, subject.as_str())?
            .context("tenant 7's")?,
        a.quads()
            .id_of(other, subject.as_str())?
            .context("the other's")?,
    );
    assert_eq!(ids.0.bucket(), ids.1.bucket(), "one bucket in both");
    Ok(())
}

/// The tenants of the live keys of the forward, reverse and predicate keyspaces, bytes 1 to 8 of each.
fn tenants_of(a: &TestChangesets) -> TestResult<Vec<u64>> {
    let snapshot = a.quads().store().snapshot(a.quads().lists().applied_ts())?;
    let mut tenants = Vec::new();
    for keyspace in [
        Keyspace::Forward,
        Keyspace::Reverse,
        Keyspace::SubjectPredicates,
        Keyspace::ObjectPredicates,
    ] {
        for item in snapshot.scan(&Scan::prefix(vec![keyspace.byte()])) {
            let item = item?;
            let bytes = item.key().get(1..9).context("a tenant's bytes")?;
            tenants.push(u64::from_be_bytes(<[u8; 8]>::try_from(bytes)?));
        }
    }
    Ok(tenants)
}

#[then(
    "each statement's keys hold its own tenant in bytes 1 to 8, a read of tenant 7 returns none of tenant 8's, and the partition owning the bucket holds both"
)]
fn tenants_apart(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let other = run_sharer()?;
    let tenants = tenants_of(&a)?;
    assert!(
        tenants.contains(&7) && tenants.contains(&other),
        "{tenants:?}"
    );
    assert!(
        tenants.iter().all(|t| *t == 7 || *t == other),
        "{tenants:?}"
    );
    let list_of = |tenant: u64| -> TestResult<Postings> {
        let id = |name: &str| -> TestResult<SystemId> {
            a.quads().id_of(tenant, ex(name).as_str())?.context("an ID")
        };
        let key = QuadKey::Forward {
            tenant,
            graph: DEFAULT_GRAPH,
            predicate: id("p")?,
            subject: id("a")?,
        }
        .to_bytes();
        Ok(a.quads().objects(&key, a.quads().lists().applied_ts())?)
    };
    let (seven, eight) = (list_of(7)?, list_of(other)?);
    let [(object_7, _)] = seven.as_slice() else {
        return Err(TestError::new("tenant 7 holds one object"));
    };
    let [(object_8, _)] = eight.as_slice() else {
        return Err(TestError::new("the other tenant holds one object"));
    };
    assert_eq!(
        object_7.as_id(),
        a.quads().id_of(7, ex("b").as_str())?,
        "tenant 7's own object"
    );
    assert_eq!(object_8.as_id(), a.quads().id_of(other, ex("b").as_str())?);
    Ok(())
}
