/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::{MetadataCommand, MetadataState};
use ultragraphdb_cluster_placement::{NodeId, PartitionId};
use ultragraphdb_core_ids::{Epoch, Tenant, TypePrefix};
use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_tool_bdd::{Context, TestResult};

/// Tenant 1's versions 1 and 2 effective at 3 and 6, version 3 pending, the last learned close 7.
fn three_versions() -> TestResult<MetadataState> {
    let mut state = MetadataState::new();
    let tenant = Tenant::new(1);
    let offer = |predecessor: u64| MetadataCommand::OfferCatalog {
        tenant,
        predecessor,
        bytes: format!("v{}", predecessor + 1).into_bytes(),
    };
    let close = |epoch: u64, versions: Vec<(Tenant, u64)>| -> TestResult<MetadataCommand> {
        Ok(MetadataCommand::LearnClose {
            placement: 0,
            epoch: Epoch::new(epoch)?,
            versions,
            evictions: Vec::new(),
        })
    };
    state.apply(&close(1, Vec::new())?)?;
    state.apply(&offer(0))?;
    state.apply(&close(2, vec![(tenant, 1)])?)?;
    state.apply(&close(3, Vec::new())?)?;
    state.apply(&close(4, Vec::new())?)?;
    state.apply(&offer(1))?;
    state.apply(&close(5, vec![(tenant, 2)])?)?;
    state.apply(&close(6, Vec::new())?)?;
    state.apply(&close(7, Vec::new())?)?;
    state.apply(&offer(2))?;
    Ok(state)
}

/// The version number in effect at `epoch` in tenant 1.
fn at(state: &MetadataState, epoch: u64) -> TestResult<Option<u64>> {
    Ok(state
        .catalog_at(Tenant::new(1), Epoch::new(epoch)?)?
        .map(|record| record.version()))
}

#[test]
fn test_a_version_is_in_effect_from_its_epoch_until_the_next() -> TestResult {
    let state = three_versions()?;
    // Before version 1's epoch, the initial catalog.
    assert_eq!(at(&state, 0)?, None);
    assert_eq!(at(&state, 2)?, None);
    assert_eq!(at(&state, 3)?, Some(1));
    assert_eq!(at(&state, 5)?, Some(1));
    assert_eq!(at(&state, 6)?, Some(2));
    // The pending version 3 takes effect at 9 at the earliest, so epoch 8, one after the last close, is answered.
    assert_eq!(at(&state, 8)?, Some(2));
    let record = state
        .catalog_at(Tenant::new(1), Epoch::new(6)?)?
        .context("version 2")?;
    assert_eq!(record.bytes(), b"v2");
    Ok(())
}

#[test]
fn test_an_epoch_past_one_after_the_last_close_is_not_answered() -> TestResult {
    let state = three_versions()?;
    assert_eq!(
        state
            .catalog_at(Tenant::new(1), Epoch::new(9)?)
            .err()
            .context("ahead")?
            .0,
        ClusterError::epoch_ahead(9, 8).0
    );
    // A tenant with no catalog answers the same way: what is pending is unknown past it.
    assert_eq!(
        state
            .catalog_at(Tenant::new(2), Epoch::new(9)?)
            .err()
            .context("ahead")?
            .0,
        ClusterError::epoch_ahead(9, 8).0
    );
    assert_eq!(state.catalog_at(Tenant::new(2), Epoch::new(8)?)?, None);
    Ok(())
}

#[test]
fn test_a_fresh_state_answers_epochs_0_and_1() -> TestResult {
    let state = MetadataState::new();
    assert_eq!(state.catalog_at(Tenant::new(1), Epoch::ZERO)?, None);
    assert_eq!(state.catalog_at(Tenant::new(1), Epoch::new(1)?)?, None);
    assert_eq!(
        state
            .catalog_at(Tenant::new(1), Epoch::new(2)?)
            .err()
            .context("a refusal")?,
        ClusterError::epoch_ahead(2, 1)
    );
    Ok(())
}

#[test]
fn test_the_versions_retained_pending_last() -> TestResult {
    let state = three_versions()?;
    let versions: Vec<(u64, Option<u64>)> = state
        .versions(Tenant::new(1))
        .iter()
        .map(|v| (v.version(), v.effective().map(Epoch::get)))
        .collect();
    assert_eq!(versions, vec![(1, Some(3)), (2, Some(6)), (3, None)]);
    assert_eq!(state.current_version(Tenant::new(1)), 3);
    assert_eq!(state.pending_versions(), vec![(Tenant::new(1), 3)]);
    assert!(state.versions(Tenant::new(2)).is_empty());
    assert!(state.tenants().is_empty(), "a version creates no tenant");
    Ok(())
}

#[test]
fn test_next_sequences_per_type() -> TestResult {
    let mut state = MetadataState::new();
    state.apply(&MetadataCommand::LeaseBlock {
        prefix: TypePrefix::STATEMENT,
        partition: PartitionId::new(1)?,
        start: 1,
        end: 50,
    })?;
    assert_eq!(state.next_sequence(TypePrefix::STATEMENT), 50);
    assert_eq!(state.next_sequence(TypePrefix::RESOURCE), 1);
    Ok(())
}

#[test]
fn test_the_placement_map_once_initialised() -> TestResult {
    let mut state = MetadataState::new();
    assert!(state.placement().is_none());
    state.apply(&MetadataCommand::InitPlacement {
        partitions: 4,
        leader: NodeId::new(2),
    })?;
    let map = state.placement().context("the map")?;
    assert_eq!(
        map.preferred_leader(PartitionId::new(4)?),
        Some(NodeId::new(2))
    );
    assert_eq!(map.preferred_leader(PartitionId::new(5)?), None);
    Ok(())
}

#[test]
fn test_a_tenants_default_entries_are_its_own_by_predicate() -> TestResult {
    let mut state = MetadataState::new();
    for (tenant, predicate) in [(2, "b"), (1, "z"), (2, "a"), (3, "a")] {
        state.apply(&MetadataCommand::RecordDefault {
            tenant: Tenant::new(tenant),
            predicate: predicate.to_string(),
            entry: format!("{tenant}{predicate}").into_bytes(),
        })?;
    }
    let entries = |tenant| -> Vec<(String, Vec<u8>)> {
        state
            .default_entries(Tenant::new(tenant))
            .map(|(predicate, entry)| (predicate.to_string(), entry.to_vec()))
            .collect()
    };
    // Tenant 2's two records, by predicate, and none of tenant 1's below them or tenant 3's above.
    assert_eq!(
        entries(2),
        vec![
            ("a".to_string(), b"2a".to_vec()),
            ("b".to_string(), b"2b".to_vec())
        ]
    );
    assert_eq!(entries(3), vec![("a".to_string(), b"3a".to_vec())]);
    assert_eq!(entries(4), Vec::new());
    Ok(())
}
