/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::{MetadataCommand, MetadataState};
use ultragraphdb_core_ids::{Epoch, Namespace};
use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_catalog::{CatalogSource, HeldVersion};

/// Namespace 4's version 1 effective at 2, version 2 pending, the last learned close 1.
fn one_effective() -> TestResult<MetadataState> {
    let mut state = MetadataState::new();
    let ns = Namespace::new(4);
    state.apply(&MetadataCommand::OfferCatalog {
        namespace: ns,
        predecessor: 0,
        bytes: b"one".to_vec(),
    })?;
    state.apply(&MetadataCommand::LearnClose {
        placement: 0,
        epoch: Epoch::new(1)?,
        versions: vec![(ns, 1)],
        evictions: Vec::new(),
    })?;
    state.apply(&MetadataCommand::OfferCatalog {
        namespace: ns,
        predecessor: 1,
        bytes: b"two".to_vec(),
    })?;
    Ok(state)
}

#[test]
fn test_a_version_at_an_epoch_is_held_as_the_partition_loads_it() -> TestResult {
    let state = one_effective()?;
    assert_eq!(state.version_at(4, 1)?, None);
    assert_eq!(
        state.version_at(4, 2)?,
        Some(HeldVersion::new(4, 1, 2, b"one".to_vec()))
    );
    assert_eq!(state.version_at(5, 2)?, None);
    Ok(())
}

#[test]
fn test_an_epoch_not_yet_answered_is_refused() -> TestResult {
    let state = one_effective()?;
    assert_eq!(
        state.version_at(4, 3).err().context("ahead")?.0,
        ClusterError::epoch_ahead(3, 2).0
    );
    // Past 42 bits, no epoch at all.
    assert_eq!(
        state.version_at(4, 1 << 42).err().context("ahead")?.0,
        ClusterError::epoch_ahead(1 << 42, 2).0
    );
    Ok(())
}

#[test]
fn test_a_version_by_number_only_once_effective() -> TestResult {
    let state = one_effective()?;
    assert_eq!(
        state.version(4, 1)?,
        Some(HeldVersion::new(4, 1, 2, b"one".to_vec()))
    );
    // Pending, never committed, another namespace.
    assert_eq!(state.version(4, 2)?, None);
    assert_eq!(state.version(4, 3)?, None);
    assert_eq!(state.version(4, 0)?, None);
    assert_eq!(state.version(5, 1)?, None);
    Ok(())
}
