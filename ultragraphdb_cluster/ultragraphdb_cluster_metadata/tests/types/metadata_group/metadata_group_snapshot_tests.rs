/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_core_ids::TypePrefix;
use ultragraphdb_error_cluster::ClusterErrorEnum;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_blocks::SequenceBlock;
use ultragraphdb_traits_log::{OrderedLog, Position};

use crate::utils::harness::{block_on, crash_and_recover, fresh, fresh_with, ns, open, pid, sync};

#[test]
fn test_a_snapshot_is_the_durable_state_at_the_applied_position() -> TestResult {
    let (_storage, mut group) = fresh()?;
    let _: (u64, Position) = group.offer_catalog(ns(), 0, b"v1")?;
    sync(&mut group)?;
    let _: (u64, Position) = group.offer_catalog(ns(), 1, b"v2")?;
    // Version 2 is not durable: the snapshot holds version 1 at position 1.
    let snapshot = group.snapshot();
    assert_eq!(snapshot.position().get(), 1);
    assert_eq!(snapshot.state(), group.state());
    assert_eq!(snapshot.state().current_version(ns()), 1);
    Ok(())
}

#[test]
fn test_a_snapshot_is_due_every_snapshot_rounds_rounds() -> TestResult {
    let (_storage, mut group) = fresh_with(MetadataConfig::new().with_snapshot_rounds(3)?)?;
    for _ in 0..2 {
        let _: (SequenceBlock, Position) = group.lease_block(TypePrefix::RESOURCE, pid(1)?, 1)?;
    }
    sync(&mut group)?;
    assert!(!group.snapshot_due());
    let _: (SequenceBlock, Position) = group.lease_block(TypePrefix::RESOURCE, pid(1)?, 1)?;
    assert!(!group.snapshot_due(), "a round not yet durable counts not");
    sync(&mut group)?;
    assert!(group.snapshot_due());
    let snapshot = group.snapshot();
    block_on(group.trim(&snapshot))??;
    assert!(!group.snapshot_due());
    for _ in 0..3 {
        let _: (SequenceBlock, Position) = group.lease_block(TypePrefix::RESOURCE, pid(1)?, 1)?;
    }
    sync(&mut group)?;
    assert!(group.snapshot_due());
    Ok(())
}

#[test]
fn test_a_recovered_group_counts_from_its_snapshot() -> TestResult {
    let config = MetadataConfig::new().with_snapshot_rounds(2)?;
    let (storage, mut group) = fresh_with(config)?;
    for _ in 0..3 {
        let _: (SequenceBlock, Position) = group.lease_block(TypePrefix::RESOURCE, pid(1)?, 1)?;
    }
    sync(&mut group)?;
    let snapshot = group.snapshot();
    let _: (SequenceBlock, Position) = group.lease_block(TypePrefix::RESOURCE, pid(1)?, 1)?;
    sync(&mut group)?;
    let mut recovered = crash_and_recover(&storage, config, Some(snapshot))?;
    // One round since the snapshot at 3.
    assert!(!recovered.snapshot_due());
    let _: (SequenceBlock, Position) = recovered.lease_block(TypePrefix::RESOURCE, pid(1)?, 1)?;
    sync(&mut recovered)?;
    assert!(recovered.snapshot_due());
    // Without a snapshot, a recovery counts from position 0.
    let fresh_again = crash_and_recover(&storage, config, None)?;
    assert!(fresh_again.snapshot_due());
    Ok(())
}

#[test]
fn test_a_trim_moves_the_logs_checkpoint_to_the_snapshot() -> TestResult {
    let (storage, mut group) = fresh()?;
    for predecessor in 0..5 {
        let _: (u64, Position) = group.offer_catalog(ns(), predecessor, b"v")?;
    }
    sync(&mut group)?;
    let snapshot = group.snapshot();
    block_on(group.trim(&snapshot))??;
    // A second trim at the same snapshot changes nothing.
    block_on(group.trim(&snapshot))??;
    drop(group);
    assert_eq!(open(&storage)?.checkpoint().get(), 5);
    Ok(())
}

#[test]
fn test_a_trim_past_the_committed_position_is_the_logs_refusal() -> TestResult {
    let (_storage, mut group) = fresh()?;
    let _: (u64, Position) = group.offer_catalog(ns(), 0, b"v")?;
    sync(&mut group)?;
    let (other_storage, mut other) = fresh()?;
    for predecessor in 0..3 {
        let _: (u64, Position) = other.offer_catalog(ns(), predecessor, b"v")?;
    }
    sync(&mut other)?;
    drop(other_storage);
    let refusal = block_on(group.trim(&other.snapshot()))?
        .err()
        .context("a refusal")?;
    assert!(matches!(refusal.0, ClusterErrorEnum::Log(_)), "{refusal:?}");
    Ok(())
}
