/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Bucket, BucketGroup, Epoch, Tenant};

use crate::types::bucket_range::BucketRange;
use crate::types::partition_id::PartitionId;
use crate::types::placement_map::PlacementMap;
use crate::types::tenant_run::TenantRun;

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
        // A group's 64 buckets lie in at most 64 runs; `BUCKETS` is 64, so its cast is lossless.
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

    /// The partitions the layout in effect at `epoch` assigns `groups` to, ascending, each once, with the first bucket of the
    /// lowest group that resolved to it: the partitions a reverse read asks, from the groups its referrer set names at `epoch`
    /// (quad-reads, "A reverse read asks the object's referrers"). The rule the partition's own path and compute both read.
    #[must_use]
    pub fn holders_of_groups(
        &self,
        groups: impl IntoIterator<Item = BucketGroup>,
        epoch: Epoch,
    ) -> Vec<(PartitionId, u32)> {
        let mut asked = Vec::new();
        let mut holders = Vec::new();
        for group in groups {
            self.group_partitions(group, epoch, &mut holders);
            asked.extend(
                holders
                    .iter()
                    .map(|holder| (*holder, group.first_bucket().get())),
            );
        }
        asked.sort_unstable();
        asked.dedup_by_key(|(holder, _)| *holder);
        asked
    }

    /// The partitions the layout in effect at `epoch` assigns a bucket of `tenant`'s run to, ascending, each once, with the
    /// first such bucket: the partitions any fan-out over the tenant asks. The rule the partition's own path and compute both
    /// read.
    #[must_use]
    pub fn holders_of_run(&self, tenant: Tenant, epoch: Epoch) -> Vec<(PartitionId, u32)> {
        let run = TenantRun::of(tenant).range();
        let mut holders: Vec<(PartitionId, u32)> = self
            .ranges_at(epoch)
            .into_iter()
            .filter(|(range, _)| range.start() < run.end() && run.start() < range.end())
            .map(|(range, owner)| (owner, range.start().max(run.start())))
            .collect();
        holders.sort_unstable();
        holders.dedup_by_key(|(owner, _)| *owner);
        holders
    }
}
