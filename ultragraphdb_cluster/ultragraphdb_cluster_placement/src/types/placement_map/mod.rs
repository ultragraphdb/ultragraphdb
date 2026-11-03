/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

pub(crate) mod layout;
mod placement_map_codec;
mod placement_map_lookup;

use std::collections::BTreeMap;

use ultragraphdb_core_ids::Epoch;
use ultragraphdb_error_cluster::ClusterError;

use crate::types::bucket_range::BucketRange;
use crate::types::namespace_run::NamespaceRun;
use crate::types::node_id::NodeId;
use crate::types::partition_id::PartitionId;
use crate::types::placement_map::layout::Layout;

/// The placement map: every one of the 2^24 buckets assigned to exactly one partition, as half-open ranges, with the
/// epoch each layout applies from, and each partition's preferred leader.
///
/// The first layout applies from epoch 0, so every epoch resolves. A change names the epoch it applies from, later
/// than the latest change's, and the complete new layout; it is refused, leaving the map unchanged, unless it assigns
/// every bucket exactly once. A layout is held normalised (neighbouring ranges of one partition merged), so two maps
/// assigning alike are equal and encode alike. The map keeps at most [`PlacementMap::LAYOUTS_MAX`] layouts, each of
/// at most [`PlacementMap::RANGES_MAX`] ranges over at most [`PlacementMap::PARTITIONS_MAX`] partitions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlacementMap {
    /// The layout applying from epoch 0.
    first: Layout,
    /// The later layouts, in the order of the epochs they apply from.
    later: Vec<Layout>,
    /// The preferred leader of every partition that has one, each holding buckets in the latest layout.
    leaders: BTreeMap<PartitionId, NodeId>,
}

impl PlacementMap {
    /// The most partitions a layout assigns buckets to, 1,024.
    pub const PARTITIONS_MAX: u32 = 1 << 10;

    /// The most ranges a layout holds once normalised, 2^18: the even split of every namespace run over the most
    /// partitions.
    pub const RANGES_MAX: u32 = 1 << 18;

    /// The most layouts the map keeps, 256.
    pub const LAYOUTS_MAX: u32 = 256;

    /// The map applying from epoch 0 that splits every namespace run over the partitions 1 to `partitions` in
    /// contiguous ranges, in partition order, whose sizes differ by at most one, the first partitions taking the larger
    /// ranges, with `leader` the preferred leader of every partition.
    ///
    /// # Errors
    ///
    /// [`ClusterError::partition_count`] when `partitions` is 0 or above [`PlacementMap::PARTITIONS_MAX`].
    pub fn even(partitions: u32, leader: NodeId) -> Result<Self, ClusterError> {
        if partitions == 0 || partitions > Self::PARTITIONS_MAX {
            return Err(ClusterError::partition_count(
                partitions,
                Self::PARTITIONS_MAX,
            ));
        }
        // Division by a count of 1 to 1,024: each run's 65,536 buckets give every partition `base` and the first
        // `larger` partitions one more.
        let base = NamespaceRun::BUCKETS / partitions;
        let larger = NamespaceRun::BUCKETS % partitions;
        let runs = 256 * partitions as usize;
        let mut layout = Layout::with_capacity(Epoch::ZERO, runs);
        for run in 0..=u8::MAX {
            let mut start = NamespaceRun::new(run).range().start();
            for index in 0..partitions {
                layout.push_run(start, PartitionId::new(index + 1)?);
                start += base + u32::from(index < larger);
            }
            debug_assert_eq!(start, NamespaceRun::new(run).range().end());
        }
        let leaders = layout.partitions.iter().map(|p| (*p, leader)).collect();
        Ok(Self {
            first: layout,
            later: Vec::new(),
            leaders,
        })
    }

    /// Applies the layout `assignments` from epoch `from`: each range assigned to its partition, in any order.
    ///
    /// # Errors
    ///
    /// Refused, with the map unchanged: [`ClusterError::change_epoch`] when `from` is not after the latest change's
    /// epoch; [`ClusterError::unassigned`] or [`ClusterError::assigned_twice`] naming the first bucket the layout
    /// leaves unassigned or assigns twice; [`ClusterError::too_many`] when the layout names more than
    /// [`PlacementMap::PARTITIONS_MAX`] partitions or holds more than [`PlacementMap::RANGES_MAX`] ranges once
    /// normalised; [`ClusterError::full`] when the map holds [`PlacementMap::LAYOUTS_MAX`] layouts.
    pub fn change(
        &mut self,
        from: Epoch,
        assignments: &[(BucketRange, PartitionId)],
    ) -> Result<(), ClusterError> {
        let latest = self.latest_change();
        if from <= latest {
            return Err(ClusterError::change_epoch(from.get(), latest.get()));
        }
        let layouts_max = Self::LAYOUTS_MAX as usize;
        if self.later.len() + 1 >= layouts_max {
            return Err(ClusterError::full("placement layouts", layouts_max));
        }
        let layout = Layout::from_assignments(from, assignments)?;
        self.leaders
            .retain(|partition, _| layout.partitions.contains(partition));
        self.later.push(layout);
        Ok(())
    }

    /// Checks the layout `assignments` as [`PlacementMap::change`] checks it, without an epoch and without applying it.
    ///
    /// # Errors
    ///
    /// [`ClusterError::unassigned`] or [`ClusterError::assigned_twice`] naming the first bucket the layout leaves
    /// unassigned or assigns twice; [`ClusterError::too_many`] past [`PlacementMap::PARTITIONS_MAX`] partitions or
    /// [`PlacementMap::RANGES_MAX`] ranges once normalised.
    pub fn check_layout(assignments: &[(BucketRange, PartitionId)]) -> Result<(), ClusterError> {
        Layout::from_assignments(Epoch::ZERO, assignments).map(|_| ())
    }

    /// How many more layouts [`PlacementMap::change`] takes before the map holds [`PlacementMap::LAYOUTS_MAX`].
    #[must_use]
    pub fn layouts_free(&self) -> usize {
        (Self::LAYOUTS_MAX as usize).saturating_sub(1 + self.later.len())
    }

    /// The layout in effect: the latest one.
    fn latest(&self) -> &Layout {
        self.later.last().unwrap_or(&self.first)
    }

    /// The layout in effect at `epoch`: the latest applying from an epoch at or before it.
    pub(crate) fn layout_at(&self, epoch: Epoch) -> &Layout {
        let applying = self.later.partition_point(|layout| layout.from <= epoch);
        applying
            .checked_sub(1)
            .and_then(|index| self.later.get(index))
            .unwrap_or(&self.first)
    }

    /// The epoch the latest layout applies from, 0 for the first.
    #[must_use]
    pub fn latest_change(&self) -> Epoch {
        self.latest().from
    }

    /// The preferred leader of `partition`, `None` when it has none.
    #[must_use]
    pub fn preferred_leader(&self, partition: PartitionId) -> Option<NodeId> {
        self.leaders.get(&partition).copied()
    }

    /// Makes `node` the preferred leader of `partition`.
    ///
    /// # Errors
    ///
    /// [`ClusterError::unknown_partition`] when the latest layout assigns `partition` no bucket.
    pub fn set_preferred_leader(
        &mut self,
        partition: PartitionId,
        node: NodeId,
    ) -> Result<(), ClusterError> {
        if !self.latest().partitions.contains(&partition) {
            return Err(ClusterError::unknown_partition(partition.get()));
        }
        let _replaced: Option<NodeId> = self.leaders.insert(partition, node);
        Ok(())
    }
}
