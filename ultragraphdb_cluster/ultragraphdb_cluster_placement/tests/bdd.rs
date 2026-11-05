/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The placement map's scenarios, bound to `features/metadata-group/`.

use cucumber::{World, then, when};
use ultragraphdb_cluster_placement::{BucketRange, NodeId, PartitionId, PlacementMap, TenantRun};
use ultragraphdb_core_ids::{Bucket, BucketGroup, Epoch, Tenant};
use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// The bucket after the last.
const END: u32 = 1 << 24;

#[derive(Debug, Default, World)]
struct Steps {
    map: Option<PlacementMap>,
    before: Option<PlacementMap>,
    tenant: Option<Tenant>,
    refusal: Option<ClusterError>,
    found: Vec<PartitionId>,
    resolved: Vec<Vec<PartitionId>>,
}

fn pid(partition: u32) -> TestResult<PartitionId> {
    Ok(PartitionId::new(partition)?)
}

fn assign(start: u32, end: u32, partition: PartitionId) -> TestResult<(BucketRange, PartitionId)> {
    Ok((BucketRange::new(start, end)?, partition))
}

/// The layout of `map` at epoch 0 with `start..end` reassigned to `partition`, or assigned to nowhere for `None`.
fn moved(
    map: &PlacementMap,
    start: u32,
    end: u32,
    partition: Option<PartitionId>,
) -> TestResult<Vec<(BucketRange, PartitionId)>> {
    let mut layout = Vec::new();
    for (range, owner) in map.ranges_at(Epoch::new(0)?) {
        if range.end() <= start || range.start() >= end {
            layout.push((range, owner));
            continue;
        }
        if range.start() < start {
            layout.push(assign(range.start(), start, owner)?);
        }
        if range.end() > end {
            layout.push(assign(end, range.end(), owner)?);
        }
    }
    if let Some(partition) = partition {
        layout.push(assign(start, end, partition)?);
    }
    Ok(layout)
}

#[when("the map is initialised for one tenant and three partitions")]
fn initialised(w: &mut Steps) -> TestResult {
    w.tenant = Some(Tenant::new(42));
    w.map = Some(PlacementMap::even(3, NodeId::new(1))?);
    Ok(())
}

#[then(
    "that tenant's 65,536 buckets are assigned in three contiguous ranges whose sizes differ by at most one, and every other bucket is assigned to a partition"
)]
fn covers_every_bucket(w: &mut Steps) -> TestResult {
    let map = w.map.as_ref().context("the map")?;
    let run = TenantRun::of(w.tenant.context("the tenant")?).range();
    let ranges = map.ranges_at(Epoch::new(0)?);
    let mine: Vec<_> = ranges
        .iter()
        .filter(|(r, _)| r.start() >= run.start() && r.end() <= run.end())
        .collect();
    assert_eq!(mine.len(), 3);
    let sizes: Vec<u32> = mine.iter().map(|(r, _)| r.buckets()).collect();
    assert_eq!(sizes.iter().sum::<u32>(), 65_536);
    let (least, most) = (sizes.iter().min(), sizes.iter().max());
    assert!(
        most.zip(least).is_some_and(|(m, l)| m - l <= 1),
        "{sizes:?}"
    );
    let distinct: std::collections::BTreeSet<_> = mine.iter().map(|(_, p)| *p).collect();
    assert_eq!(distinct.len(), 3);
    // Every bucket of the space once: the ranges follow each other from 0 to the end.
    let mut next = 0;
    for (range, _) in &ranges {
        assert_eq!(range.start(), next);
        next = range.end();
    }
    assert_eq!(next, END);
    Ok(())
}

#[when("a change moves the range 100..200 from partition 1 to nowhere")]
fn moved_to_nowhere(w: &mut Steps) -> TestResult {
    let mut map = PlacementMap::even(3, NodeId::new(1))?;
    assert_eq!(map.partition_of(Bucket::new(100)?, Epoch::new(0)?), pid(1)?);
    let layout = moved(&map, 100, 200, None)?;
    w.before = Some(map.clone());
    w.refusal = map.change(Epoch::new(1)?, &layout).err();
    w.map = Some(map);
    Ok(())
}

#[then("the change is refused with an error naming bucket 100, and the map is unchanged")]
fn refused_naming_100(w: &mut Steps) -> TestResult {
    let refusal = w.refusal.as_ref().context("a refusal")?;
    assert_eq!(refusal.0, ClusterErrorEnum::Unassigned { bucket: 100 });
    assert!(refusal.to_string().contains("bucket 100"), "{refusal}");
    assert_eq!(w.map, w.before);
    Ok(())
}

#[when("partition 2 holds the range 21,846..43,691 and bucket 30,000 is looked up")]
fn bucket_looked_up(w: &mut Steps) -> TestResult {
    let map = PlacementMap::even(3, NodeId::new(1))?;
    assert!(
        map.ranges_at(Epoch::new(0)?)
            .contains(&assign(21_846, 43_691, pid(2)?)?)
    );
    w.found
        .push(map.partition_of(Bucket::new(30_000)?, Epoch::new(0)?));
    Ok(())
}

#[then("the lookup answers partition 2")]
fn answers_partition_2(w: &mut Steps) -> TestResult {
    assert_eq!(w.found, vec![pid(2)?]);
    Ok(())
}

#[when(
    "a change moving buckets 21,888..21,952 from partition 2 to partition 3 applies from epoch 21, and the bucket group holding those 64 buckets is resolved at epochs 20 and 21"
)]
fn group_resolved(w: &mut Steps) -> TestResult {
    let mut map = PlacementMap::even(3, NodeId::new(1))?;
    let layout = moved(&map, 21_888, 21_952, Some(pid(3)?))?;
    map.change(Epoch::new(21)?, &layout)?;
    let group = BucketGroup::of(Bucket::new(21_888)?);
    let range = BucketRange::of_group(group);
    assert_eq!((range.start(), range.end()), (21_888, 21_952));
    for epoch in [20, 21] {
        let mut into = Vec::new();
        map.group_partitions(group, Epoch::new(epoch)?, &mut into);
        w.resolved.push(into);
    }
    Ok(())
}

#[then("it resolves to partition 2 at epoch 20 and to partition 3 at epoch 21")]
fn resolves_by_epoch(w: &mut Steps) -> TestResult {
    assert_eq!(w.resolved, vec![vec![pid(2)?], vec![pid(3)?]]);
    Ok(())
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
