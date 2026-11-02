/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::Bucket;
use ultragraphdb_error_cluster::ClusterError;

use crate::types::bucket_range::BucketRange;
use crate::types::namespace_run::NamespaceRun;

/// A bucket group: 64 contiguous buckets, a bucket's top 18 bits. A referrer set names the groups of the subjects
/// holding edges to its entity, and the placement map resolves a group to partitions at an epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BucketGroup(u32);

impl BucketGroup {
    /// The buckets of a group, 64.
    pub const BUCKETS: u32 = 64;

    /// The number of groups, 2^18.
    pub const COUNT: u32 = 1 << 18;

    /// The group `group`.
    ///
    /// # Errors
    ///
    /// [`ClusterError::bucket_group_range`] when `group` is not below [`BucketGroup::COUNT`].
    pub fn new(group: u32) -> Result<Self, ClusterError> {
        if group < Self::COUNT {
            Ok(Self(group))
        } else {
            Err(ClusterError::bucket_group_range(group))
        }
    }

    /// The group `bucket` lies in: its top 18 bits.
    #[must_use]
    pub fn of(bucket: Bucket) -> Self {
        // 24 bits less the 6 of a group's 64 buckets.
        Self(bucket.get() >> 6)
    }

    /// The group as a number below 2^18.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0
    }

    /// The group's 64 buckets as a range.
    #[must_use]
    pub fn range(self) -> BucketRange {
        let start = self.0 * Self::BUCKETS;
        BucketRange::from_checked(start, start + Self::BUCKETS)
    }

    /// The namespace run the group lies in.
    #[must_use]
    pub fn run(self) -> NamespaceRun {
        // 18 bits less the 10 of a run's 1,024 groups leave the run's 8.
        let [.., top] = (self.0 >> 10).to_be_bytes();
        NamespaceRun::new(top)
    }
}
