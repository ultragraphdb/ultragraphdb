/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Bucket, BucketGroup, Epoch};

use crate::types::bucket_range::BucketRange;
use crate::types::partition_id::PartitionId;
use crate::types::placement_map::PlacementMap;

impl PlacementMap {
    /// The partition the layout in effect at `epoch` assigns `bucket` to.
    #[must_use]
    pub fn partition_of(&self, bucket: Bucket, epoch: Epoch) -> PartitionId {
        let layout = self.layout_at(epoch);
        layout.partition_at(layout.run_of(bucket.get()))
    }

    /// The partitions the layout in effect at `epoch` assigns `group`'s 64 buckets to, written into `into` in
    /// ascending order, each once; `into` is cleared first, so a caller reuses one buffer.
    pub fn group_partitions(&self, group: BucketGroup, epoch: Epoch, into: &mut Vec<PartitionId>) {
        into.clear();
        let layout = self.layout_at(epoch);
        let range = BucketRange::of_group(group);
        let first = layout.run_of(range.start());
        // A group's 64 buckets lie in at most 64 runs.
        for index in first..first + BucketGroup::BUCKETS as usize {
            if index > first && layout.end_of(index - 1) >= range.end() {
                break;
            }
            into.push(layout.partition_at(index));
        }
        into.sort_unstable();
        into.dedup();
    }

    /// The layout in effect at `epoch` as its normalised ranges, in bucket order, each with its partition.
    #[must_use]
    pub fn ranges_at(&self, epoch: Epoch) -> Vec<(BucketRange, PartitionId)> {
        let layout = self.layout_at(epoch);
        layout
            .runs
            .iter()
            .enumerate()
            .map(|(index, (start, partition))| {
                (
                    BucketRange::from_checked(*start, layout.end_of(index)),
                    *partition,
                )
            })
            .collect()
    }
}
