/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, Tenant};
use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_catalog::DefaultEntry;
use ultragraphdb_traits_log::Position;

use crate::utils::harness::{epoch, fresh, learn_through, pid, sync, tenant};

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
