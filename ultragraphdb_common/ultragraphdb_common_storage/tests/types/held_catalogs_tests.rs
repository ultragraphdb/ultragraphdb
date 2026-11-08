/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_storage::HeldCatalogs;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_catalog::{CatalogSource, HeldVersion};

#[test]
fn test_up_to_gives_a_tenants_versions_in_effect_by_an_epoch_in_order() {
    let mut held = HeldCatalogs::default();
    for (tenant, version, effective) in [(1, 2, 8), (1, 1, 5), (1, 3, 9), (2, 1, 1)] {
        held.hold(HeldVersion::new(tenant, version, effective, Vec::new()));
    }
    let versions = |tenant, epoch| -> Vec<u64> {
        held.up_to(tenant, epoch)
            .map(HeldVersion::version)
            .collect()
    };
    assert_eq!(versions(1, 8), [1, 2], "in order, the later one left out");
    assert_eq!(versions(1, 4), Vec::<u64>::new());
    assert_eq!(versions(2, 9), [1], "another tenant's are its own");
}

/// Versions 1, 2 and 3 of tenant 1, taking effect at epochs 5, 8 and 9, and version 1 of tenant 2 at epoch 1.
fn held() -> TestResult<HeldCatalogs> {
    let mut held = HeldCatalogs::default();
    for (tenant, version, effective) in [(1, 2, 8), (1, 1, 5), (1, 3, 9), (2, 1, 1)] {
        let contents = vec![u8::try_from(version)?];
        held.hold(HeldVersion::new(tenant, version, effective, contents));
    }
    Ok(held)
}

/// The version in effect at an epoch is the one of the highest effective epoch at or below it, and none before the first.
#[test]
fn test_version_at_is_the_latest_in_effect_by_the_epoch() -> TestResult {
    let held = held()?;
    let at = |tenant, epoch| -> TestResult<Option<u64>> {
        Ok(held.version_at(tenant, epoch)?.map(|held| held.version()))
    };
    assert_eq!(at(1, 8)?, Some(2), "taking effect at the epoch itself");
    assert_eq!(at(1, 7)?, Some(1));
    assert_eq!(at(1, 100)?, Some(3));
    assert_eq!(at(1, 4)?, None, "before any took effect");
    assert_eq!(at(2, 1)?, Some(1), "another tenant's are its own");
    assert_eq!(at(3, 100)?, None);
    Ok(())
}

/// A version is found by its tenant and number, with what it holds.
#[test]
fn test_version_is_found_by_its_tenant_and_number() -> TestResult {
    let held = held()?;
    let found = held.version(1, 2)?.context("version 2 of tenant 1")?;
    assert_eq!(
        (found.tenant(), found.version(), found.effective()),
        (1, 2, 8)
    );
    assert_eq!(found.contents(), [2]);
    assert!(held.version(2, 2)?.is_none());
    assert!(held.version(3, 1)?.is_none());
    Ok(())
}
