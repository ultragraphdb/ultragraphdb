/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_partition::{Partition, PartitionError};
use ultragraphdb_cluster_placement::{BucketRange, NamespaceRun, NodeId, PlacementMap};
use ultragraphdb_core_ids::{Bucket, Namespace};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_store_sim::{SimListCache, SimStore, SteppedExecutor};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_blocks::TestBlockSource;
use ultragraphdb_transactions_txn::TxnError;

use crate::utils::cluster::{Cluster, NS, epoch, insert, pid};

type Part = Partition<SimStore, SimListCache<CachedList>, SteppedExecutor, TestBlockSource>;

/// Partition `id` owns exactly the ranges the map gives it: the first and last bucket of each of its ranges, and of
/// no other partition's.
#[test]
fn test_a_partition_owns_the_ranges_the_map_gives_it() -> TestResult {
    let placement = PlacementMap::even(3, NodeId::new(1))?;
    let at = epoch(1)?;
    for (range, owner) in placement.ranges_at(at) {
        for id in 1..=3 {
            let owned = Part::owned(pid(id)?, &placement, at).context("the map's ranges")?;
            for bucket in [range.start(), range.end() - 1] {
                assert_eq!(
                    owned.owns(Bucket::new(bucket)?),
                    owner.get() == id,
                    "bucket {bucket} of partition {owner:?}, asked of {id}"
                );
            }
        }
    }
    Ok(())
}

/// A partition takes its own bucket and refuses another's naming the bucket and its owner, by the map at the epoch.
#[test]
fn test_routing_names_the_owner_of_a_bucket_elsewhere() -> TestResult {
    let cluster = Cluster::even(2)?;
    let at = cluster.closed();
    let ranges = cluster.part(1)?.placement().ranges_at(at);
    let (own, _) = ranges
        .iter()
        .find(|(_, p)| p.get() == 1)
        .context("a range of partition 1")?;
    let (other, _) = ranges
        .iter()
        .find(|(_, p)| p.get() == 2)
        .context("a range of partition 2")?;
    let part = cluster.part(1)?;
    part.route(Bucket::new(own.start())?, at)?;
    assert_eq!(
        part.route(Bucket::new(other.start())?, at),
        Err(PartitionError::wrong_partition(other.start(), 2))
    );
    Ok(())
}

/// One partition owns every namespace's run; of two even partitions neither owns a run; a partition given a run whole
/// owns it.
#[test]
fn test_a_partition_owns_a_run_only_when_it_holds_all_of_it() -> TestResult {
    let at = epoch(1)?;
    let alone = Cluster::even(1)?;
    assert!(alone.part(1)?.owns_run(NS, at));
    let spread = Cluster::even(2)?;
    assert!(!spread.part(1)?.owns_run(NS, at));
    assert!(!spread.part(2)?.owns_run(NS, at));
    let run = NamespaceRun::of(Namespace::new(NS)).range();
    let mut placement = PlacementMap::even(2, NodeId::new(1))?;
    placement.change(epoch(2)?, &whole_run_to_one(&placement, run)?)?;
    let moved = Cluster::bare(&placement)?;
    let later = epoch(2)?;
    assert!(moved.part(1)?.owns_run(NS, later));
    assert!(!moved.part(2)?.owns_run(NS, later));
    assert!(!moved.part(1)?.owns_run(NS, at), "not before the change");
    Ok(())
}

/// The layout of `placement` at epoch 1 with `run` given whole to partition 1.
fn whole_run_to_one(
    placement: &PlacementMap,
    run: BucketRange,
) -> TestResult<Vec<(BucketRange, ultragraphdb_cluster_placement::PartitionId)>> {
    let mut assignments: Vec<_> = placement
        .ranges_at(epoch(1)?)
        .into_iter()
        .filter(|(range, _)| range.end() <= run.start() || range.start() >= run.end())
        .collect();
    assignments.push((run, pid(1)?));
    Ok(assignments)
}

/// A transaction begun at a partition is bound to it: writing a subject another partition owns aborts it, naming both.
#[test]
fn test_a_transaction_begun_at_a_partition_is_bound_to_it() -> TestResult {
    let cluster = Cluster::even(2)?;
    let foreign = cluster.name_in(2, "s")?;
    let result = cluster.read(1, |txn| {
        txn.write(
            &crate::utils::cluster::changeset(vec![insert(&foreign, "knows", "x")]),
            0,
        )
    })?;
    assert_eq!(result, Err(TxnError::cross_partition(1, 2)));
    Ok(())
}

/// A Drop is local only when the partition owns every bucket it reaches: alone, every Drop is; of two partitions a
/// Drop of the namespace's data or of everything reaches the other, named.
#[test]
fn test_a_drop_reaching_another_partition_is_refused() -> TestResult {
    let at = epoch(1)?;
    let alone = Cluster::even(1)?;
    alone.part(1)?.check_drop(Some(NS), at)?;
    alone.part(1)?.check_drop(None, at)?;
    let spread = Cluster::even(2)?;
    assert_eq!(
        spread.part(1)?.check_drop(Some(NS), at),
        Err(PartitionError::cross_partition(1, 2))
    );
    assert_eq!(
        spread.part(2)?.check_drop(None, at),
        Err(PartitionError::cross_partition(2, 1))
    );
    Ok(())
}
