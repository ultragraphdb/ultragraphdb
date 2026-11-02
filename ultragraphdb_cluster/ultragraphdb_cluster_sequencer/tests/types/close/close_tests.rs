/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_sequencer::{CatalogVersion, Close};
use ultragraphdb_core_ids::{Epoch, Namespace};
use ultragraphdb_error_cluster::ClusterErrorEnum;
use ultragraphdb_tool_bdd::{Context, TestResult};

fn versions(count: u64) -> Vec<CatalogVersion> {
    (1..=count)
        .map(|v| CatalogVersion::new(Namespace::new(1), v))
        .collect()
}

#[test]
fn test_the_limits() {
    assert_eq!(Close::CATALOG_MAX, 256);
    assert_eq!(Close::EVICTIONS_MAX, 4_096);
}

#[test]
fn test_a_close_keeps_its_fields() -> TestResult {
    let catalog = vec![CatalogVersion::new(Namespace::new(3), 7)];
    let close = Close::new(
        Epoch::new(39)?,
        1_000,
        catalog.clone(),
        vec![0xAB, u128::MAX],
    )?;
    assert_eq!(close.epoch().get(), 39);
    assert_eq!(close.time_us(), 1_000);
    assert_eq!(close.catalog(), catalog.as_slice());
    assert_eq!(close.evictions(), [0xAB, u128::MAX]);
    Ok(())
}

#[test]
fn test_announced_versions_take_effect_one_epoch_later() -> TestResult {
    let close = Close::new(Epoch::new(39)?, 0, versions(1), vec![])?;
    assert_eq!(close.effective_epoch().map(Epoch::get), Some(40));
    let first = Close::new(Epoch::new(1)?, 0, vec![], vec![])?;
    assert_eq!(first.effective_epoch().map(Epoch::get), Some(2));
    Ok(())
}

#[test]
fn test_no_epoch_follows_the_last_close() -> TestResult {
    let last = Close::new(Epoch::MAX, 0, vec![], vec![])?;
    assert_eq!(last.effective_epoch(), None);
    Ok(())
}

#[test]
fn test_the_most_versions_and_evictions_are_taken() -> TestResult {
    let evictions: Vec<u128> = (0..4_096).collect();
    let close = Close::new(Epoch::new(2)?, 0, versions(256), evictions)?;
    assert_eq!(close.catalog().len(), 256);
    assert_eq!(close.evictions().len(), 4_096);
    Ok(())
}

#[test]
fn test_one_version_too_many_is_refused() -> TestResult {
    let refusal = Close::new(Epoch::new(2)?, 0, versions(257), vec![])
        .err()
        .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::TooMany {
            what: "catalog versions in a close",
            count: 257,
            max: 256
        }
    );
    Ok(())
}

#[test]
fn test_one_eviction_too_many_is_refused() -> TestResult {
    let refusal = Close::new(Epoch::new(2)?, 0, vec![], (0..4_097).collect())
        .err()
        .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::TooMany {
            what: "session evictions in a close",
            count: 4_097,
            max: 4_096
        }
    );
    Ok(())
}
