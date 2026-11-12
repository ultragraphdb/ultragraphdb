/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::id_error::IdError;
use crate::types::bucket::Bucket;

/// A bucket group: 64 contiguous buckets, a bucket's top 18 bits. A referring partition marks an entity per bucket
/// group of its referring subjects, a referrer set names the groups of the subjects holding edges to its entity, and
/// the placement map resolves a group to partitions at an epoch.
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
    /// [`IdError::bucket_group_range`] when `group` is not below [`BucketGroup::COUNT`].
    pub fn new(group: u32) -> Result<Self, IdError> {
        if group < Self::COUNT {
            Ok(Self(group))
        } else {
            Err(IdError::bucket_group_range(group))
        }
    }

    /// The group `bucket` lies in: its top 18 bits.
    #[must_use]
    pub fn of(bucket: Bucket) -> Self {
        // 24 bits less the 6 of a group's 64 buckets.
        let group = Self(bucket.get() >> 6);
        debug_assert!(group.0 < Self::COUNT, "a bucket's group holds 18 bits");
        group
    }

    /// The group as a number below 2^18.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0
    }

    /// The group's first bucket, `64 × group`: its buckets are the 64 from it.
    #[must_use]
    pub fn first_bucket(self) -> Bucket {
        // Below 2^18, so 64 times it is below 2^24 and every value is a bucket.
        self.0
            .checked_mul(Self::BUCKETS)
            .and_then(|first| Bucket::new(first).ok())
            .unwrap_or(Bucket::MAX)
    }
}
