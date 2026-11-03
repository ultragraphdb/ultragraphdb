/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Bucket, BucketGroup, Namespace};

use crate::types::bucket_range::BucketRange;

/// A namespace's run: the 65,536 buckets whose top 8 bits are the namespace's hash
/// ([`Namespace::bucket_bits`]), 2^10 bucket groups of 64. Two namespaces whose hashes share their top 8 bits share
/// a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NamespaceRun(u8);

impl NamespaceRun {
    /// The buckets of a run, 2^16.
    pub const BUCKETS: u32 = 1 << 16;

    /// The bucket groups of a run, 2^10.
    pub const GROUPS: u32 = 1 << 10;

    /// The run `group` lies in.
    #[must_use]
    pub fn of_group(group: BucketGroup) -> Self {
        Self::of_bucket(group.first_bucket())
    }

    /// The run whose buckets' top 8 bits are `bits`.
    #[must_use]
    pub fn new(bits: u8) -> Self {
        Self(bits)
    }

    /// The run of `namespace`'s entities.
    #[must_use]
    pub fn of(namespace: Namespace) -> Self {
        Self(namespace.bucket_bits())
    }

    /// The run `bucket` lies in.
    #[must_use]
    pub fn of_bucket(bucket: Bucket) -> Self {
        // A bucket holds 24 bits, so its top 8 are its bits from 16 on.
        let [_, top, ..] = bucket.get().to_be_bytes();
        Self(top)
    }

    /// The top 8 bits of the run's buckets.
    #[must_use]
    pub fn get(self) -> u8 {
        self.0
    }

    /// The run's buckets as a range.
    #[must_use]
    pub fn range(self) -> BucketRange {
        let start = u32::from(self.0) << 16;
        BucketRange::from_checked(start, start + Self::BUCKETS)
    }
}
