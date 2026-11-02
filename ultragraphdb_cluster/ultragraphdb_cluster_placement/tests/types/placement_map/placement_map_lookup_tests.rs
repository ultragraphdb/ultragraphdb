/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::{BucketGroup, BucketRange, NodeId, PartitionId, PlacementMap};
use ultragraphdb_core_ids::{Bucket, Epoch};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::reference_model::{END, owner};

fn pid(partition: u32) -> TestResult<PartitionId> {
    Ok(PartitionId::new(partition)?)
}

fn at(epoch: u64) -> TestResult<Epoch> {
    Ok(Epoch::new(epoch)?)
}

fn assign(start: u32, end: u32, partition: u32) -> TestResult<(BucketRange, PartitionId)> {
    Ok((BucketRange::new(start, end)?, pid(partition)?))
}

fn partition_of(map: &PlacementMap, bucket: u32, epoch: u64) -> TestResult<u32> {
    Ok(map.partition_of(Bucket::new(bucket)?, at(epoch)?).get())
}

fn group_partitions(map: &PlacementMap, group: u32, epoch: u64) -> TestResult<Vec<u32>> {
    let mut into = Vec::new();
    map.group_partitions(BucketGroup::new(group)?, at(epoch)?, &mut into);
    Ok(into.iter().map(|p| p.get()).collect())
}

/// A map whose epoch-7 layout is 0..100 on 4, 100..130 on 2, 130..160 on 4, the rest on 1.
fn changed() -> TestResult<PlacementMap> {
    let mut map = PlacementMap::even(3, NodeId::new(1))?;
    map.change(
        at(7)?,
        &[
            assign(0, 100, 4)?,
            assign(100, 130, 2)?,
            assign(130, 160, 4)?,
            assign(160, END, 1)?,
        ],
    )?;
    Ok(map)
}

#[test]
fn test_a_bucket_resolves_at_the_edges_of_its_ranges() -> TestResult {
    let map = PlacementMap::even(3, NodeId::new(1))?;
    // Run 0: 0..21,846 on 1, 21,846..43,691 on 2, 43,691..65,536 on 3; run 1 starts again on 1.
    for (bucket, partition) in [
        (0, 1),
        (21_845, 1),
        (21_846, 2),
        (43_690, 2),
        (43_691, 3),
        (65_535, 3),
        (65_536, 1),
        (END - 1, 3),
    ] {
        assert_eq!(partition_of(&map, bucket, 0)?, partition, "bucket {bucket}");
    }
    Ok(())
}

#[test]
fn test_a_bucket_resolves_by_the_layout_in_effect_at_the_epoch() -> TestResult {
    let map = changed()?;
    for epoch in [0, 6] {
        assert_eq!(partition_of(&map, 0, epoch)?, 1);
        assert_eq!(partition_of(&map, 21_846, epoch)?, 2);
    }
    for epoch in [7, 8, 4_398_046_511_103] {
        assert_eq!(partition_of(&map, 0, epoch)?, 4);
        assert_eq!(partition_of(&map, 99, epoch)?, 4);
        assert_eq!(partition_of(&map, 100, epoch)?, 2);
        assert_eq!(partition_of(&map, 160, epoch)?, 1);
        assert_eq!(partition_of(&map, 21_846, epoch)?, 1);
    }
    Ok(())
}

#[test]
fn test_every_bucket_of_a_run_matches_the_reference_model() -> TestResult {
    // Run 0 bucket by bucket against the plain model of the epoch-7 layout.
    let layout = changed()?.ranges_at(at(7)?);
    let map = changed()?;
    for bucket in 0..65_536 {
        let model = owner(&layout, bucket).map(PartitionId::get);
        assert_eq!(
            Some(partition_of(&map, bucket, 7)?),
            model,
            "bucket {bucket}"
        );
    }
    Ok(())
}

#[test]
fn test_a_group_in_one_range_resolves_to_its_partition() -> TestResult {
    let map = PlacementMap::even(3, NodeId::new(1))?;
    assert_eq!(group_partitions(&map, 0, 0)?, vec![1]);
    assert_eq!(group_partitions(&map, 342, 0)?, vec![2]);
    assert_eq!(group_partitions(&map, 262_143, 0)?, vec![3]);
    Ok(())
}

#[test]
fn test_a_group_across_a_boundary_resolves_to_both_partitions() -> TestResult {
    // Group 341 holds 21,824..21,888, across the boundary at 21,846.
    let map = PlacementMap::even(3, NodeId::new(1))?;
    assert_eq!(group_partitions(&map, 341, 0)?, vec![1, 2]);
    Ok(())
}

#[test]
fn test_a_group_names_each_partition_once_in_order() -> TestResult {
    // Group 1 holds 64..128 at epoch 7: 64..100 on 4, 100..128 on 2. Group 2 holds 128..192: 128..130 on 2,
    // 130..160 on 4, then 1.
    let map = changed()?;
    assert_eq!(group_partitions(&map, 1, 7)?, vec![2, 4]);
    assert_eq!(group_partitions(&map, 2, 7)?, vec![1, 2, 4]);
    // Group 0 alone, before the change and after it.
    assert_eq!(group_partitions(&map, 0, 6)?, vec![1]);
    assert_eq!(group_partitions(&map, 0, 7)?, vec![4]);
    Ok(())
}

#[test]
fn test_a_partition_repeated_within_a_group_is_named_once() -> TestResult {
    let mut map = PlacementMap::even(1, NodeId::new(1))?;
    map.change(
        at(1)?,
        &[
            assign(0, 10, 3)?,
            assign(10, 20, 2)?,
            assign(20, 30, 3)?,
            assign(30, END, 3)?,
        ],
    )?;
    assert_eq!(group_partitions(&map, 0, 1)?, vec![2, 3]);
    Ok(())
}

#[test]
fn test_group_partitions_clears_its_buffer() -> TestResult {
    let map = PlacementMap::even(3, NodeId::new(1))?;
    let mut into = vec![pid(9)?, pid(8)?];
    map.group_partitions(BucketGroup::new(0)?, at(0)?, &mut into);
    assert_eq!(into, vec![pid(1)?]);
    Ok(())
}

#[test]
fn test_ranges_at_covers_every_bucket_once_in_order() -> TestResult {
    let ranges = changed()?.ranges_at(at(7)?);
    assert_eq!(ranges.first().copied(), Some(assign(0, 100, 4)?));
    let mut next = 0;
    for (range, _) in &ranges {
        assert_eq!(range.start(), next);
        next = range.end();
    }
    assert_eq!(next, END);
    // Neighbours differ: 0..100 on 4, 100..130 on 2, 130..160 on 4, 160.. on 1.
    assert_eq!(ranges.len(), 4);
    Ok(())
}
