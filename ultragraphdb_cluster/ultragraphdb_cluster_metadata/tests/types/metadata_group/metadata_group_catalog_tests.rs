/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::{CatalogCheck, MetadataConfig};
use ultragraphdb_core_ids::{Epoch, Tenant};
use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_catalog::DefaultEntry;
use ultragraphdb_traits_log::Position;
use ultragraphdb_transactions_log::SimStorage;

use crate::utils::harness::{
    Group, crash_and_recover, epoch, fresh, learn_through, node, pid, sync, tenant,
};

#[test]
fn test_an_offer_numbers_the_version_after_its_predecessor() -> TestResult {
    let (_storage, mut group) = fresh()?;
    assert_eq!(group.offer_catalog(tenant(), 0, b"a")?.0, 1);
    assert_eq!(group.offer_catalog(tenant(), 1, b"b")?.0, 2);
    assert_eq!(group.offer_catalog(Tenant::new(8), 0, b"c")?.0, 1);
    let refusal = group
        .offer_catalog(tenant(), 1, b"x")
        .err()
        .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterError::stale_catalog(tenant().get(), 1, 2).0
    );
    Ok(())
}

#[test]
fn test_a_default_entry_is_recorded_once() -> TestResult {
    let (_storage, mut group) = fresh()?;
    let first = DefaultEntry::new(tenant().get(), "score".to_string(), b"first".to_vec());
    let position = group.record_default_entry(&first)?.context("recorded")?;
    assert_eq!(position.get(), 1);
    // Found among the commands appended, durable or not: nothing is appended again.
    let again = DefaultEntry::new(tenant().get(), "score".to_string(), b"second".to_vec());
    assert_eq!(group.record_default_entry(&again)?, None);
    sync(&mut group)?;
    assert_eq!(group.record_default_entry(&again)?, None);
    assert_eq!(
        group.state().default_entry(tenant(), "score"),
        Some(&b"first"[..])
    );
    assert_eq!(group.applied().get(), 1);
    // Another tenant or predicate is another record.
    let other = DefaultEntry::new(9, "score".to_string(), b"other".to_vec());
    assert_eq!(
        group.record_default_entry(&other)?.map(|p| p.get()),
        Some(2)
    );
    Ok(())
}

#[test]
fn test_a_learned_close_assigns_its_versions_the_epoch_after_it() -> TestResult {
    let (_storage, mut group) = fresh()?;
    let _: (u64, Position) = group.offer_catalog(tenant(), 0, b"a")?;
    sync(&mut group)?;
    let position = group.learn_close(epoch(39)?, &[(tenant(), 1)], 0, &[])?;
    assert_eq!(position.map(|p| p.get()), Some(2));
    sync(&mut group)?;
    let record = group
        .state()
        .catalog_at(tenant(), epoch(40)?)?
        .context("version 1")?;
    assert_eq!(record.effective(), Some(epoch(40)?));
    assert_eq!(group.state().catalog_at(tenant(), epoch(39)?)?, None);
    Ok(())
}

#[test]
fn test_a_close_learned_already_changes_nothing() -> TestResult {
    let (_storage, mut group) = fresh()?;
    learn_through(&mut group, 3)?;
    let applied = group.applied();
    assert_eq!(group.learn_close(epoch(3)?, &[], 0, &[])?, None);
    assert_eq!(group.learn_close(epoch(1)?, &[], 0, &[])?, None);
    assert_eq!(group.learn_close(Epoch::ZERO, &[], 0, &[])?, None);
    assert_eq!(group.applied(), applied);
    // A close learned but not yet durable is learned already too.
    let _: Option<Position> = group.learn_close(epoch(4)?, &[], 0, &[])?;
    assert_eq!(group.learn_close(epoch(4)?, &[], 0, &[])?, None);
    Ok(())
}

#[test]
fn test_a_close_skipping_an_epoch_is_refused() -> TestResult {
    let (_storage, mut group) = fresh()?;
    learn_through(&mut group, 3)?;
    let refusal = group
        .learn_close(epoch(5)?, &[], 0, &[])
        .err()
        .context("a refusal")?;
    assert_eq!(refusal.0, ClusterError::epoch_ahead(5, 4).0);
    Ok(())
}

#[test]
fn test_a_close_past_the_sequencers_limits_is_refused() -> TestResult {
    let (_storage, mut group) = fresh()?;
    let versions = vec![(tenant(), 1); 257];
    let refusal = group
        .learn_close(epoch(1)?, &versions, 0, &[])
        .err()
        .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterError::too_many("catalog versions in a close", 257, 256).0
    );
    let evictions = vec![1; 4_097];
    let refusal_2 = group
        .learn_close(epoch(1)?, &[], 0, &evictions)
        .err()
        .context("a refusal_2")?;
    assert_eq!(
        refusal_2.0,
        ClusterError::too_many("session evictions in a close", 4_097, 4_096).0
    );
    Ok(())
}

#[test]
fn test_a_report_is_recorded() -> TestResult {
    let (_storage, mut group) = fresh()?;
    let position = group.report_untrimmed(pid(3)?, epoch(17)?)?;
    assert_eq!(position.get(), 1);
    sync(&mut group)?;
    assert_eq!(group.state().untrimmed(pid(3)?), Some(epoch(17)?));
    Ok(())
}

/// A group of two partitions holding version 1 of `tenant`, being checked, its epoch assigned.
fn being_checked() -> TestResult<(SimStorage, Group)> {
    let (storage, mut group) = fresh()?;
    let _init = group.init_placement(2, node())?;
    let _offered = group.offer_checked(tenant(), 0, b"tight")?;
    sync(&mut group)?;
    learn_through(&mut group, 1)?;
    Ok((storage, group))
}

fn check_of(group: &Group, version: u64) -> TestResult<CatalogCheck> {
    Ok(group
        .state()
        .version_record(tenant(), version)
        .context("the version")?
        .check()
        .clone())
}

#[test]
fn test_a_report_before_the_epoch_is_assigned_appends_nothing() -> TestResult {
    let (_storage, mut group) = fresh()?;
    let _init = group.init_placement(1, node())?;
    let _offered = group.offer_checked(tenant(), 0, b"tight")?;
    sync(&mut group)?;
    assert_eq!(group.report_check(tenant(), 1, pid(1)?, false)?, None);
    assert_eq!(group.state().checked_version(tenant()), Some(1));
    Ok(())
}

#[test]
fn test_a_report_of_a_version_not_checked_appends_nothing() -> TestResult {
    let (_storage, mut group) = fresh()?;
    let _offered = group.offer_catalog(tenant(), 0, b"plain")?;
    sync(&mut group)?;
    learn_through(&mut group, 1)?;
    assert_eq!(group.report_check(tenant(), 1, pid(1)?, false)?, None);
    assert_eq!(group.report_check(tenant(), 2, pid(1)?, true)?, None);
    assert_eq!(check_of(&group, 1)?, CatalogCheck::Settled);
    Ok(())
}

#[test]
fn test_a_refusal_stands_against_a_later_pass() -> TestResult {
    let (_storage, mut group) = being_checked()?;
    assert!(group.report_check(tenant(), 1, pid(1)?, true)?.is_some());
    assert_eq!(group.report_check(tenant(), 1, pid(2)?, false)?, None);
    sync(&mut group)?;
    assert!(
        matches!(
            check_of(&group, 1)?,
            CatalogCheck::Checking { refused: true, .. }
        ),
        "{:?}",
        check_of(&group, 1)?
    );
    Ok(())
}

#[test]
fn test_a_restoring_version_needs_one_being_checked() -> TestResult {
    let (_storage, mut group) = fresh()?;
    let _offered = group.offer_catalog(tenant(), 0, b"plain")?;
    sync(&mut group)?;
    let refusal = group
        .offer_restoring(tenant(), 1, 1, b"back")
        .err()
        .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterError::contradicts("a restoring version withdraws one being checked").0
    );
    assert_eq!(group.state().current_version(tenant()), 1);
    Ok(())
}

#[test]
fn test_what_a_client_reads_skips_a_withdrawn_version_and_stops_at_one_checked() -> TestResult {
    let (_storage, mut group) = fresh()?;
    let _init = group.init_placement(1, node())?;
    let _first = group.offer_catalog(tenant(), 0, b"first")?;
    sync(&mut group)?;
    learn_through(&mut group, 1)?;
    let _tight = group.offer_checked(tenant(), 1, b"tight")?;
    let _built = group.offer_catalog(tenant(), 2, b"dropped from it")?;
    sync(&mut group)?;
    learn_through(&mut group, 3)?;
    let at = epoch(4)?;
    let read = |of: &Group| of.state().settled_at(tenant(), at).map(|r| r.version());
    assert_eq!(read(&group), Some(1), "one checked hides those after it");
    let _restoring = group.offer_restoring(tenant(), 3, 2, b"back")?;
    sync(&mut group)?;
    assert_eq!(read(&group), Some(3), "the withdrawn one is left out");
    assert_eq!(check_of(&group, 2)?, CatalogCheck::Withdrawn);
    Ok(())
}

#[test]
fn test_a_version_before_one_being_checked_is_not_released() -> TestResult {
    let (_storage, mut group) = fresh()?;
    let _init = group.init_placement(1, node())?;
    let _first = group.offer_catalog(tenant(), 0, b"first")?;
    let _second = group.offer_catalog(tenant(), 1, b"second")?;
    sync(&mut group)?;
    learn_through(&mut group, 1)?;
    let _tight = group.offer_checked(tenant(), 2, b"tight")?;
    sync(&mut group)?;
    learn_through(&mut group, 3)?;
    let _reported = group.report_untrimmed(pid(1)?, epoch(100)?)?;
    sync(&mut group)?;
    let held: Vec<u64> = group
        .state()
        .versions(tenant())
        .iter()
        .map(|r| r.version())
        .collect();
    assert_eq!(
        held,
        vec![2, 3],
        "the one below what restores goes, not that"
    );
    Ok(())
}

#[test]
fn test_a_snapshot_keeps_a_check_as_it_stands() -> TestResult {
    let (storage, mut group) = being_checked()?;
    let _passed = group.report_check(tenant(), 1, pid(1)?, false)?;
    sync(&mut group)?;
    let snapshot = group.snapshot();
    let mut bytes = Vec::new();
    snapshot.encode(&mut bytes);
    let decoded = ultragraphdb_cluster_metadata::MetadataSnapshot::decode(&bytes)?;
    assert_eq!(decoded, snapshot);
    let recovered = crash_and_recover(&storage, MetadataConfig::new(), Some(decoded))?;
    assert_eq!(recovered.state(), group.state());
    assert_eq!(recovered.state().checked_version(tenant()), Some(1));
    Ok(())
}
