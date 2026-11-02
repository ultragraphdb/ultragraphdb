/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU32;

use ultragraphdb_error_cluster::ClusterError;

/// A partition, the one owner of the facts of the buckets the placement map assigns it. Partitions are numbered from
/// 1, so 0 names none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PartitionId(NonZeroU32);

impl PartitionId {
    /// Partition 1, the first.
    pub(crate) const FIRST: Self = Self(NonZeroU32::MIN);

    /// The partition `partition`.
    ///
    /// # Errors
    ///
    /// [`ClusterError::zero_partition`] when `partition` is 0.
    pub fn new(partition: u32) -> Result<Self, ClusterError> {
        NonZeroU32::new(partition)
            .map(Self)
            .ok_or_else(ClusterError::zero_partition)
    }

    /// The partition as a number, at least 1.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0.get()
    }
}
