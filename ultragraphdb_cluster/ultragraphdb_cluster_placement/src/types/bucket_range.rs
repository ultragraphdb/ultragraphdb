/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Bucket, BucketGroup};
use ultragraphdb_error_cluster::ClusterError;

/// A half-open range of bucket numbers, `start..end`: at least one bucket, none past the last, 2^24 − 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BucketRange {
    start: u32,
    end: u32,
}

impl BucketRange {
    /// The bucket after the last, 2^24: the largest `end` of a range.
    pub const END: u32 = 1 << 24;

    /// The 64 buckets of `group`.
    #[must_use]
    pub fn of_group(group: BucketGroup) -> Self {
        let start = group.first_bucket().get();
        Self::from_checked(start, start + BucketGroup::BUCKETS)
    }

    /// The range `start..end`.
    ///
    /// # Errors
    ///
    /// [`ClusterError::bucket_range`] when `start` is not below `end` or `end` is above [`BucketRange::END`].
    pub fn new(start: u32, end: u32) -> Result<Self, ClusterError> {
        if start < end && end <= Self::END {
            Ok(Self { start, end })
        } else {
            Err(ClusterError::bucket_range(start, end))
        }
    }

    /// The first bucket of the range.
    #[must_use]
    pub fn start(self) -> u32 {
        self.start
    }

    /// The bucket after the range's last.
    #[must_use]
    pub fn end(self) -> u32 {
        self.end
    }

    /// The number of buckets in the range, at least 1.
    #[must_use]
    pub fn buckets(self) -> u32 {
        // start < end by construction, so the difference is at least 1 and cannot wrap.
        self.end - self.start
    }

    /// Whether `bucket` lies in the range.
    #[must_use]
    pub fn contains(self, bucket: Bucket) -> bool {
        (self.start..self.end).contains(&bucket.get())
    }

    /// The range `start..end` of bounds a caller computed within the buckets: below [`BucketRange::END`] for `start`,
    /// at most it for `end`, and `start` below `end`.
    pub(crate) fn from_checked(start: u32, end: u32) -> Self {
        debug_assert!(start < end, "a range holds a bucket");
        debug_assert!(end <= Self::END, "a range ends at the last bucket at most");
        Self { start, end }
    }
}
