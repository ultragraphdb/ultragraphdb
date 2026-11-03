/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::{MetadataCommand, MetadataState};
use ultragraphdb_cluster_placement::{NodeId, PartitionId};
use ultragraphdb_core_ids::{Epoch, Namespace, TypePrefix};
use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_tool_bdd::{Context, TestResult};

/// Namespace 1's versions 1 and 2 effective at 3 and 6, version 3 pending, the last learned close 7.
fn three_versions() -> TestResult<MetadataState> {
    let mut state = MetadataState::new();
    let ns = Namespace::new(1);
    let offer = |predecessor: u64| MetadataCommand::OfferCatalog {
        namespace: ns,
        predecessor,
        bytes: format!("v{}", predecessor + 1).into_bytes(),
    };
    let close = |epoch: u64, versions: Vec<(Namespace, u64)>| -> TestResult<MetadataCommand> {
        Ok(MetadataCommand::LearnClose {
            epoch: Epoch::new(epoch)?,
            versions,
            evictions: Vec::new(),
        })
    };
    state.apply(&close(1, Vec::new())?)?;
    state.apply(&offer(0))?;
    state.apply(&close(2, vec![(ns, 1)])?)?;
    state.apply(&close(3, Vec::new())?)?;
    state.apply(&close(4, Vec::new())?)?;
    state.apply(&offer(1))?;
    state.apply(&close(5, vec![(ns, 2)])?)?;
    state.apply(&close(6, Vec::new())?)?;
    state.apply(&close(7, Vec::new())?)?;
    state.apply(&offer(2))?;
    Ok(state)
}

/// The version number in effect at `epoch` in namespace 1.
fn at(state: &MetadataState, epoch: u64) -> TestResult<Option<u64>> {
    Ok(state
        .catalog_at(Namespace::new(1), Epoch::new(epoch)?)?
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
        .catalog_at(Namespace::new(1), Epoch::new(6)?)?
        .context("version 2")?;
    assert_eq!(record.bytes(), b"v2");
    Ok(())
}

#[test]
fn test_an_epoch_past_one_after_the_last_close_is_not_answered() -> TestResult {
    let state = three_versions()?;
    assert_eq!(
        state
            .catalog_at(Namespace::new(1), Epoch::new(9)?)
            .err()
            .context("ahead")?
            .0,
        ClusterError::epoch_ahead(9, 8).0
    );
    // A namespace with no catalog answers the same way: what is pending is unknown past it.
    assert_eq!(
        state
            .catalog_at(Namespace::new(2), Epoch::new(9)?)
            .err()
            .context("ahead")?
            .0,
        ClusterError::epoch_ahead(9, 8).0
    );
    assert_eq!(state.catalog_at(Namespace::new(2), Epoch::new(8)?)?, None);
    Ok(())
}

#[test]
fn test_a_fresh_state_answers_epochs_0_and_1() -> TestResult {
    let state = MetadataState::new();
    assert_eq!(state.catalog_at(Namespace::new(1), Epoch::ZERO)?, None);
    assert_eq!(state.catalog_at(Namespace::new(1), Epoch::new(1)?)?, None);
    assert!(state.catalog_at(Namespace::new(1), Epoch::new(2)?).is_err());
    Ok(())
}

#[test]
fn test_the_versions_retained_pending_last() -> TestResult {
    let state = three_versions()?;
    let versions: Vec<(u64, Option<u64>)> = state
        .versions(Namespace::new(1))
        .iter()
        .map(|v| (v.version(), v.effective().map(Epoch::get)))
        .collect();
    assert_eq!(versions, vec![(1, Some(3)), (2, Some(6)), (3, None)]);
    assert_eq!(state.current_version(Namespace::new(1)), 3);
    assert_eq!(state.pending_versions(), vec![(Namespace::new(1), 3)]);
    assert!(state.versions(Namespace::new(2)).is_empty());
    assert_eq!(state.namespaces(), vec![Namespace::new(1)]);
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
