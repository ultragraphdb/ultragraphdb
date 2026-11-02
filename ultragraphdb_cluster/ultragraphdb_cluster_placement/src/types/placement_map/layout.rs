/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_core_ids::Epoch;
use ultragraphdb_error_cluster::ClusterError;

use crate::types::bucket_range::BucketRange;
use crate::types::partition_id::PartitionId;
use crate::types::placement_map::PlacementMap;

/// One layout of the map, normalised: the epoch it applies from and every bucket's partition as runs, each starting at
/// its first bucket and ending at the next run's start or at the last bucket's end. The first run starts at 0, the
/// starts rise strictly, and neighbouring runs belong to different partitions, so one assignment has one layout. The
/// set of its partitions is derived from the runs once, when the layout is built, and never changes after.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Layout {
    /// The epoch the layout applies from.
    pub from: Epoch,
    /// Each run's first bucket and its partition, in bucket order.
    pub runs: Vec<(u32, PartitionId)>,
    /// The partitions the runs name.
    pub partitions: BTreeSet<PartitionId>,
}

impl Layout {
    /// An empty layout from `from`, for runs pushed in bucket order, with room for `runs`.
    pub fn with_capacity(from: Epoch, runs: usize) -> Self {
        Self {
            from,
            runs: Vec::with_capacity(runs),
            partitions: BTreeSet::new(),
        }
    }

    /// Appends the run of `partition` from `start`, after every run so far, merged with the last run when that is
    /// `partition`'s too.
    pub fn push_run(&mut self, start: u32, partition: PartitionId) {
        debug_assert!(self.runs.last().is_none_or(|(last, _)| *last < start));
        debug_assert!(
            !self.runs.is_empty() || start == 0,
            "the first run starts at 0"
        );
        if self.runs.last().is_none_or(|(_, last)| *last != partition) {
            self.runs.push((start, partition));
            let _new: bool = self.partitions.insert(partition);
        }
    }

    /// The layout from `from` that `assignments` give, in any order: refused naming the first bucket left
    /// unassigned or assigned twice, then past the limits of runs and partitions.
    pub fn from_assignments(
        from: Epoch,
        assignments: &[(BucketRange, PartitionId)],
    ) -> Result<Self, ClusterError> {
        let mut sorted: Vec<(u32, u32, PartitionId)> = assignments
            .iter()
            .map(|(range, partition)| (range.start(), range.end(), *partition))
            .collect();
        sorted.sort_unstable();
        let mut layout = Self::with_capacity(from, sorted.len());
        // Before any fault the ranges tile 0..expected exactly, so the first fault is at `expected` or at a start.
        let mut expected = 0;
        for (start, end, partition) in sorted {
            if start > expected {
                return Err(ClusterError::unassigned(expected));
            }
            if start < expected {
                return Err(ClusterError::assigned_twice(start));
            }
            layout.push_run(start, partition);
            expected = end;
        }
        if expected < BucketRange::END {
            return Err(ClusterError::unassigned(expected));
        }
        layout.check_limits()?;
        Ok(layout)
    }

    /// Refuses a layout past [`PlacementMap::RANGES_MAX`] runs or [`PlacementMap::PARTITIONS_MAX`] partitions.
    pub fn check_limits(&self) -> Result<(), ClusterError> {
        let ranges_max = PlacementMap::RANGES_MAX as usize;
        if self.runs.len() > ranges_max {
            return Err(ClusterError::too_many(
                "ranges in a layout",
                self.runs.len(),
                ranges_max,
            ));
        }
        let partitions_max = PlacementMap::PARTITIONS_MAX as usize;
        if self.partitions.len() > partitions_max {
            return Err(ClusterError::too_many(
                "partitions in a layout",
                self.partitions.len(),
                partitions_max,
            ));
        }
        Ok(())
    }

    /// The index of the run holding `bucket`: the last run starting at or before it.
    pub fn run_of(&self, bucket: u32) -> usize {
        // The first run starts at 0, so at least one run starts at or before any bucket.
        self.runs
            .partition_point(|(start, _)| *start <= bucket)
            .saturating_sub(1)
    }

    /// The partition of run `index`.
    pub fn partition_at(&self, index: usize) -> PartitionId {
        debug_assert!(index < self.runs.len(), "a run index names a run");
        self.runs
            .get(index)
            .map_or(PartitionId::FIRST, |(_, partition)| *partition)
    }

    /// The bucket after run `index`'s last: the next run's start, or the end of the buckets.
    pub fn end_of(&self, index: usize) -> u32 {
        self.runs
            .get(index + 1)
            .map_or(BucketRange::END, |(start, _)| *start)
    }
}
