/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::{BucketRange, NodeId, PartitionId, PlacementMap};
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_error_cluster::ClusterErrorEnum;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::reference_model::{END, Rng, first_fault, owner};

fn pid(partition: u32) -> TestResult<PartitionId> {
    Ok(PartitionId::new(partition)?)
}

fn at(epoch: u64) -> TestResult<Epoch> {
    Ok(Epoch::new(epoch)?)
}

fn assign(start: u32, end: u32, partition: u32) -> TestResult<(BucketRange, PartitionId)> {
    Ok((BucketRange::new(start, end)?, pid(partition)?))
}

/// The layout of `map` at `epoch` with `start..end` reassigned to `partition`, or removed for `None`.
fn moved(
    map: &PlacementMap,
    epoch: u64,
    start: u32,
    end: u32,
    partition: Option<u32>,
) -> TestResult<Vec<(BucketRange, PartitionId)>> {
    let mut layout = Vec::new();
    for (range, owner) in map.ranges_at(at(epoch)?) {
        let (lo, hi) = (range.start(), range.end());
        if hi <= start || lo >= end {
            layout.push((range, owner));
            continue;
        }
        if lo < start {
            layout.push(assign(lo, start, owner.get())?);
        }
        if hi > end {
            layout.push(assign(end, hi, owner.get())?);
        }
    }
    if let Some(partition) = partition {
        layout.push(assign(start, end, partition)?);
    }
    Ok(layout)
}

#[test]
fn test_the_limits() {
    assert_eq!(PlacementMap::PARTITIONS_MAX, 1_024);
    // 256 runs times 1,024 partitions.
    assert_eq!(PlacementMap::RANGES_MAX, 262_144);
    assert_eq!(PlacementMap::LAYOUTS_MAX, 256);
}

#[test]
fn test_an_even_map_over_no_partition_or_too_many_is_refused() -> TestResult {
    for count in [0, 1_025, u32::MAX] {
        let refusal = PlacementMap::even(count, NodeId::new(1))
            .err()
            .context("a refusal")?;
        assert_eq!(
            refusal.0,
            ClusterErrorEnum::PartitionCount { count, max: 1_024 }
        );
    }
    Ok(())
}

#[test]
fn test_one_partition_holds_every_bucket_in_one_range() -> TestResult {
    let map = PlacementMap::even(1, NodeId::new(4))?;
    let ranges = map.ranges_at(at(0)?);
    assert_eq!(ranges, vec![assign(0, END, 1)?]);
    assert_eq!(map.latest_change().get(), 0);
    assert_eq!(map.preferred_leader(pid(1)?), Some(NodeId::new(4)));
    Ok(())
}

#[test]
fn test_three_partitions_split_every_run_in_order() -> TestResult {
    let map = PlacementMap::even(3, NodeId::new(9))?;
    let ranges = map.ranges_at(at(0)?);
    // 65,536 = 21,846 + 21,845 + 21,845: the first partition takes the larger range, in every one of the 256 runs.
    assert_eq!(ranges.len(), 768);
    for (run, chunk) in (0u32..).zip(ranges.chunks(3)) {
        let base = run << 16;
        assert_eq!(
            chunk,
            [
                assign(base, base + 21_846, 1)?,
                assign(base + 21_846, base + 43_691, 2)?,
                assign(base + 43_691, base + 65_536, 3)?,
            ]
        );
    }
    for partition in 1..=3 {
        assert_eq!(map.preferred_leader(pid(partition)?), Some(NodeId::new(9)));
    }
    assert_eq!(map.preferred_leader(pid(4)?), None);
    Ok(())
}

#[test]
fn test_the_most_partitions_take_64_buckets_of_every_run() -> TestResult {
    // 65,536 / 1,024 = 64: every range one bucket group, 2^18 ranges, the limit.
    let map = PlacementMap::even(1_024, NodeId::new(1))?;
    let ranges = map.ranges_at(at(0)?);
    assert_eq!(ranges.len(), 262_144);
    assert!(ranges.iter().all(|(range, _)| range.buckets() == 64));
    assert_eq!(ranges.last().map(|(_, p)| p.get()), Some(1_024));
    Ok(())
}

#[test]
fn test_two_partitions_split_a_run_in_halves() -> TestResult {
    let map = PlacementMap::even(2, NodeId::new(1))?;
    let ranges = map.ranges_at(at(0)?);
    assert_eq!(ranges.len(), 512);
    assert_eq!(ranges.first().copied(), Some(assign(0, 32_768, 1)?));
    assert_eq!(ranges.get(1).copied(), Some(assign(32_768, 65_536, 2)?));
    Ok(())
}

#[test]
fn test_a_change_applies_from_its_epoch() -> TestResult {
    let mut map = PlacementMap::even(3, NodeId::new(1))?;
    let layout = moved(&map, 0, 21_888, 21_952, Some(3))?;
    map.change(at(21)?, &layout)?;
    assert_eq!(map.latest_change().get(), 21);
    assert_eq!(
        map.ranges_at(at(20)?),
        PlacementMap::even(3, NodeId::new(1))?.ranges_at(at(0)?)
    );
    let after = map.ranges_at(at(21)?);
    assert!(after.contains(&assign(21_846, 21_888, 2)?));
    assert!(after.contains(&assign(21_888, 21_952, 3)?));
    assert!(after.contains(&assign(21_952, 43_691, 2)?));
    assert_eq!(map.ranges_at(at(1_000)?), after);
    Ok(())
}

#[test]
fn test_a_change_is_normalised_whatever_its_order_and_cuts() -> TestResult {
    // The whole space as four ranges of partition 1, out of order, is the one range of an even map over 1 partition.
    let mut map = PlacementMap::even(2, NodeId::new(1))?;
    let layout = [
        assign(500, END, 1)?,
        assign(0, 100, 1)?,
        assign(300, 500, 1)?,
        assign(100, 300, 1)?,
    ];
    map.change(at(1)?, &layout)?;
    assert_eq!(map.ranges_at(at(1)?), vec![assign(0, END, 1)?]);
    Ok(())
}

#[test]
fn test_a_change_not_after_the_latest_is_refused() -> TestResult {
    let mut map = PlacementMap::even(2, NodeId::new(1))?;
    let layout = map.ranges_at(at(0)?);
    let refusal = map.change(at(0)?, &layout).err().context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::ChangeEpoch {
            epoch: 0,
            latest: 0
        }
    );
    map.change(at(5)?, &layout)?;
    for epoch in [5, 4] {
        let later = map.change(at(epoch)?, &layout).err().context("a refusal")?;
        assert_eq!(later.0, ClusterErrorEnum::ChangeEpoch { epoch, latest: 5 });
    }
    Ok(())
}

#[test]
fn test_a_gap_is_refused_naming_its_first_bucket() -> TestResult {
    let mut map = PlacementMap::even(1, NodeId::new(1))?;
    let before = map.clone();
    let cases = [
        (vec![assign(1, END, 1)?], 0),
        (vec![assign(0, 100, 1)?, assign(200, END, 1)?], 100),
        (vec![assign(0, END - 1, 1)?], END - 1),
        (vec![], 0),
    ];
    for (layout, bucket) in cases {
        let refusal = map.change(at(1)?, &layout).err().context("a refusal")?;
        assert_eq!(refusal.0, ClusterErrorEnum::Unassigned { bucket });
        assert_eq!(map, before);
    }
    Ok(())
}

#[test]
fn test_an_overlap_is_refused_naming_its_first_bucket() -> TestResult {
    let mut map = PlacementMap::even(1, NodeId::new(1))?;
    let before = map.clone();
    let cases = [
        (vec![assign(0, 150, 1)?, assign(100, END, 2)?], 100),
        (vec![assign(0, END, 1)?, assign(0, END, 1)?], 0),
        (vec![assign(0, END, 1)?, assign(END - 1, END, 2)?], END - 1),
        // The overlap at 50 comes before the gap at 300.
        (
            vec![
                assign(0, 100, 1)?,
                assign(50, 200, 2)?,
                assign(300, END, 1)?,
            ],
            50,
        ),
    ];
    for (layout, bucket) in cases {
        let refusal = map.change(at(1)?, &layout).err().context("a refusal")?;
        assert_eq!(refusal.0, ClusterErrorEnum::AssignedTwice { bucket });
        assert_eq!(map, before);
    }
    Ok(())
}

#[test]
fn test_a_gap_before_an_overlap_is_named_first() -> TestResult {
    let mut map = PlacementMap::even(1, NodeId::new(1))?;
    let layout = [
        assign(0, 100, 1)?,
        assign(150, 300, 2)?,
        assign(200, END, 1)?,
    ];
    let refusal = map.change(at(1)?, &layout).err().context("a refusal")?;
    assert_eq!(refusal.0, ClusterErrorEnum::Unassigned { bucket: 100 });
    Ok(())
}

#[test]
fn test_changes_agree_with_the_reference_model() -> TestResult {
    // 200 seeded layouts, each valid or broken by one of three faults; the map's verdict must be the model's, bucket
    // by bucket, and a valid layout's lookups the model's owner at every cut and at random buckets.
    let mut rng = Rng::new(0x9E37_79B9_7F4A_7C15);
    for case in 0..200u64 {
        let layout = random_layout(&mut rng, case)?;
        let mut map = PlacementMap::even(1, NodeId::new(1))?;
        let verdict = map.change(at(1)?, &layout).err().map(|e| e.0);
        assert_eq!(verdict, first_fault(&layout), "case {case}");
        if verdict.is_some() {
            continue;
        }
        let probes = layout
            .iter()
            .flat_map(|(r, _)| [r.start(), r.end().saturating_sub(1), r.end()])
            .chain((0..32).map(|_| rng.below(END)))
            .filter(|&b| b < END);
        for bucket in probes {
            let found = map.partition_of(ultragraphdb_core_ids::Bucket::new(bucket)?, at(1)?);
            assert_eq!(
                Some(found),
                owner(&layout, bucket),
                "case {case}, bucket {bucket}"
            );
        }
    }
    Ok(())
}

/// A layout of up to 12 ranges over partitions 1 to 4; for three cases in four, one fault: a range dropped, a range
/// grown into its neighbour, or a range repeated.
fn random_layout(rng: &mut Rng, case: u64) -> TestResult<Vec<(BucketRange, PartitionId)>> {
    let cuts_count = rng.below(12);
    let mut cuts: Vec<u32> = (0..cuts_count).map(|_| 1 + rng.below(END - 1)).collect();
    cuts.sort_unstable();
    cuts.dedup();
    let mut layout = Vec::new();
    let mut start = 0;
    for end in cuts.into_iter().chain(std::iter::once(END)) {
        layout.push(assign(start, end, 1 + rng.below(4))?);
        start = end;
    }
    let index = rng.below(u32::try_from(layout.len())?) as usize;
    match case % 4 {
        1 => {
            let _dropped = layout.remove(index);
        }
        2 if index + 1 < layout.len() => {
            let (range, partition) = *layout.get(index).context("a range")?;
            let next_end = layout.get(index + 1).context("its neighbour")?.0.end();
            let grown = BucketRange::new(
                range.start(),
                next_end.min(range.end() + 1 + rng.below(1_000)),
            )?;
            *layout.get_mut(index).context("a range")? = (grown, partition);
        }
        3 => {
            let repeated = *layout.get(index).context("a range")?;
            layout.push(repeated);
        }
        _ => {}
    }
    Ok(layout)
}

#[test]
fn test_a_change_naming_too_many_partitions_is_refused() -> TestResult {
    // 1,025 single-bucket ranges of distinct partitions, then the rest.
    let mut layout: Vec<_> = (0..1_025u32)
        .map(|b| assign(b, b + 1, b + 1))
        .collect::<TestResult<_>>()?;
    layout.push(assign(1_025, END, 1)?);
    let mut map = PlacementMap::even(1, NodeId::new(1))?;
    let refusal = map.change(at(1)?, &layout).err().context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::TooMany {
            what: "partitions in a layout",
            count: 1_025,
            max: 1_024
        }
    );
    // 1,024 partitions are taken.
    layout.truncate(layout.len() - 2);
    layout.push(assign(1_024, END, 1)?);
    map.change(at(1)?, &layout)?;
    Ok(())
}

#[test]
fn test_a_change_past_the_most_ranges_is_refused() -> TestResult {
    // The even split over 1,024 partitions holds 2^18 ranges; cutting its first range in two over two partitions makes
    // one more.
    let mut map = PlacementMap::even(1_024, NodeId::new(1))?;
    let mut layout = map.ranges_at(at(0)?);
    // The limit itself is taken.
    map.change(at(1)?, &layout)?;
    let (first, _) = layout.remove(0);
    layout.push(assign(first.start(), first.start() + 1, 2)?);
    layout.push(assign(first.start() + 1, first.end(), 1)?);
    let refusal = map.change(at(2)?, &layout).err().context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::TooMany {
            what: "ranges in a layout",
            count: 262_145,
            max: 262_144
        }
    );
    Ok(())
}

#[test]
fn test_the_map_keeps_at_most_256_layouts() -> TestResult {
    let mut map = PlacementMap::even(2, NodeId::new(1))?;
    let layout = map.ranges_at(at(0)?);
    // The first layout and 255 changes.
    for epoch in 1..=255 {
        map.change(at(epoch)?, &layout)?;
    }
    let refusal = map.change(at(256)?, &layout).err().context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::Full {
            what: "placement layouts",
            capacity: 256
        }
    );
    assert_eq!(map.latest_change().get(), 255);
    Ok(())
}

#[test]
fn test_a_preferred_leader_is_set_for_a_partition_holding_buckets() -> TestResult {
    let mut map = PlacementMap::even(3, NodeId::new(1))?;
    map.set_preferred_leader(pid(2)?, NodeId::new(7))?;
    assert_eq!(map.preferred_leader(pid(2)?), Some(NodeId::new(7)));
    assert_eq!(map.preferred_leader(pid(1)?), Some(NodeId::new(1)));
    let refusal = map
        .set_preferred_leader(pid(4)?, NodeId::new(7))
        .err()
        .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::UnknownPartition { partition: 4 }
    );
    Ok(())
}

#[test]
fn test_a_change_drops_the_leader_of_a_partition_it_empties_and_adds_none() -> TestResult {
    let mut map = PlacementMap::even(2, NodeId::new(1))?;
    // Partition 2 leaves, partition 5 arrives.
    map.change(at(3)?, &[assign(0, 100, 5)?, assign(100, END, 1)?])?;
    assert_eq!(map.preferred_leader(pid(1)?), Some(NodeId::new(1)));
    assert_eq!(map.preferred_leader(pid(2)?), None);
    assert_eq!(map.preferred_leader(pid(5)?), None);
    map.set_preferred_leader(pid(5)?, NodeId::new(3))?;
    assert_eq!(map.preferred_leader(pid(5)?), Some(NodeId::new(3)));
    let refusal = map
        .set_preferred_leader(pid(2)?, NodeId::new(3))
        .err()
        .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::UnknownPartition { partition: 2 }
    );
    Ok(())
}
