/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::{
    MetadataCommand, MetadataConfig, MetadataSnapshot, MetadataState,
};
use ultragraphdb_cluster_placement::NodeId;
use ultragraphdb_core_ids::TypePrefix;
use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_log::Position;
use ultragraphdb_transactions_log::SimStorage;

use crate::utils::harness::{epoch, fresh, fresh_with, node, pid, range, recover, sync};

#[test]
fn test_leases_follow_the_high_water_mark_per_type() -> TestResult {
    let (_storage, mut group) = fresh()?;
    let (a, _) = group.lease_block(TypePrefix::RESOURCE, pid(1)?, 1)?;
    let (b, _) = group.lease_block(TypePrefix::STATEMENT, pid(1)?, 4)?;
    let (c, _) = group.lease_block(TypePrefix::RESOURCE, pid(2)?, 1_048_576)?;
    assert_eq!((a.start().get(), a.end()), (1, 2));
    assert_eq!((b.start().get(), b.end()), (1, 5));
    assert_eq!((c.start().get(), c.end()), (2, 1_048_578));
    Ok(())
}

#[test]
fn test_a_block_of_0_or_past_the_maximum_is_refused() -> TestResult {
    let (_storage, mut group) = fresh_with(MetadataConfig::new().with_block_size_max(10)?)?;
    let zero = group
        .lease_block(TypePrefix::RESOURCE, pid(1)?, 0)
        .err()
        .context("refused")?;
    assert_eq!(zero.0, ClusterError::block_size(0, 10).0);
    let over = group
        .lease_block(TypePrefix::RESOURCE, pid(1)?, 11)
        .err()
        .context("refused")?;
    assert_eq!(over.0, ClusterError::block_size(11, 10).0);
    let (block, _) = group.lease_block(TypePrefix::RESOURCE, pid(1)?, 10)?;
    assert_eq!(block.count(), 10);
    Ok(())
}

#[test]
fn test_a_block_past_the_last_sequence_is_refused() -> TestResult {
    // A group recovered from a snapshot whose `Resource` high-water mark is 2^64 − 6: five sequences are left.
    let mut state = MetadataState::new();
    state.apply(&MetadataCommand::LeaseBlock {
        prefix: TypePrefix::RESOURCE,
        partition: pid(1)?,
        start: 1,
        end: u64::MAX - 5,
    })?;
    let storage = SimStorage::new(4);
    let snapshot = MetadataSnapshot::new(Position::ZERO, state);
    let mut group = recover(&storage, MetadataConfig::new(), Some(snapshot))?;
    let refusal = group
        .lease_block(TypePrefix::RESOURCE, pid(1)?, 6)
        .err()
        .context("refused")?;
    assert_eq!(refusal.0, ClusterError::sequences_spent(1).0);
    let (last, _) = group.lease_block(TypePrefix::RESOURCE, pid(1)?, 5)?;
    assert_eq!((last.start().get(), last.end()), (u64::MAX - 5, u64::MAX));
    let refusal_2 = group
        .lease_block(TypePrefix::RESOURCE, pid(1)?, 1)
        .err()
        .context("refused")?;
    assert_eq!(refusal_2.0, ClusterError::sequences_spent(1).0);
    // Another type is not spent.
    let (other, _) = group.lease_block(TypePrefix::STATEMENT, pid(1)?, 1)?;
    assert_eq!(other.start().get(), 1);
    Ok(())
}

#[test]
fn test_the_placement_map_through_the_group() -> TestResult {
    let (_storage, mut group) = fresh()?;
    let none = group
        .change_placement(epoch(1)?, &[(range(0, 16_777_216)?, pid(1)?)])
        .err()
        .context("refused")?;
    assert_eq!(none.0, ClusterError::no_placement().0);
    let none_2 = group
        .set_preferred_leader(pid(1)?, node())
        .err()
        .context("refused")?;
    assert_eq!(none_2.0, ClusterError::no_placement().0);
    let _: Position = group.init_placement(2, node())?;
    let twice = group.init_placement(2, node()).err().context("refused")?;
    assert_eq!(twice.0, ClusterError::full("placement maps", 1).0);
    let _: Position = group.set_preferred_leader(pid(2)?, NodeId::new(7))?;
    let unknown = group
        .set_preferred_leader(pid(3)?, NodeId::new(7))
        .err()
        .context("refused")?;
    assert_eq!(unknown.0, ClusterError::unknown_partition(3).0);
    let _: Position = group.change_placement(epoch(4)?, &[(range(0, 16_777_216)?, pid(2)?)])?;
    let stale = group
        .change_placement(epoch(4)?, &[(range(0, 16_777_216)?, pid(1)?)])
        .err()
        .context("refused")?;
    assert_eq!(stale.0, ClusterError::change_epoch(4, 4).0);
    sync(&mut group)?;
    let map = group.state().placement().context("the map")?;
    assert_eq!(map.latest_change(), epoch(4)?);
    assert_eq!(map.preferred_leader(pid(2)?), Some(NodeId::new(7)));
    // Partition 1 holds no bucket from epoch 4, so its leader is dropped.
    assert_eq!(map.preferred_leader(pid(1)?), None);
    Ok(())
}

#[test]
fn test_an_even_map_over_no_partition_is_refused() -> TestResult {
    let (_storage, mut group) = fresh()?;
    let refusal = group.init_placement(0, node()).err().context("refused")?;
    assert_eq!(refusal.0, ClusterError::partition_count(0, 1_024).0);
    let refusal_2 = group
        .init_placement(1_025, node())
        .err()
        .context("refused")?;
    assert_eq!(refusal_2.0, ClusterError::partition_count(1_025, 1_024).0);
    let too_many: Vec<_> = (0..262_145_u32)
        .map(|i| Ok((range(i, i + 1)?, pid(1)?)))
        .collect::<ultragraphdb_tool_bdd::TestResult<_>>()?;
    let _: Position = group.init_placement(1, node())?;
    let refusal_3 = group
        .change_placement(epoch(1)?, &too_many)
        .err()
        .context("refused")?;
    assert_eq!(
        refusal_3.0,
        ClusterError::too_many("ranges of a placement change", 262_145, 262_144).0
    );
    Ok(())
}

#[test]
fn test_a_placement_change_of_more_than_16384_ranges_commits_across_rounds() -> TestResult {
    // 20,000 ranges of 64 buckets alternating partitions 1 and 2, and the rest to partition 1: 240 KB, two chunks.
    let (storage, mut group) = fresh()?;
    let _: Position = group.init_placement(2, node())?;
    let mut assignments: Vec<_> = (0..20_000_u32)
        .map(|i| Ok((range(i * 64, (i + 1) * 64)?, pid(1 + i % 2)?)))
        .collect::<TestResult<_>>()?;
    assignments.push((range(20_000 * 64, 16_777_216)?, pid(1)?));
    let position = group.change_placement(epoch(3)?, &assignments)?;
    assert_eq!(position.get(), 4);
    sync(&mut group)?;
    let map = group.state().placement().context("the map")?;
    assert_eq!(map.ranges_at(epoch(3)?).len(), 20_001);
    let bucket = ultragraphdb_core_ids::Bucket::new(64 * 333 + 5)?;
    assert_eq!(map.partition_of(bucket, epoch(3)?), pid(2)?);
    let recovered =
        crate::utils::harness::crash_and_recover(&storage, MetadataConfig::new(), None)?;
    assert_eq!(recovered.state(), group.state());
    Ok(())
}
