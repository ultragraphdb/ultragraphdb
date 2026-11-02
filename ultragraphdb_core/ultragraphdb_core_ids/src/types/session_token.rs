/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::epoch::Epoch;

/// A session token, which a committed transaction's reply carries: the partition the commit landed in, its log
/// position there and the epoch of its commit version. A later read or transaction carrying it sees that commit and
/// every commit before it in that partition.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionToken {
    partition: u32,
    position: u64,
    epoch: Epoch,
}

impl SessionToken {
    /// The token of a commit in `partition` at log `position`, with a version of `epoch`.
    #[must_use]
    pub fn new(partition: u32, position: u64, epoch: Epoch) -> Self {
        Self {
            partition,
            position,
            epoch,
        }
    }

    /// The partition the commit landed in.
    #[must_use]
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// The commit's log position in its partition.
    #[must_use]
    pub fn position(&self) -> u64 {
        self.position
    }

    /// The epoch of the commit's version.
    #[must_use]
    pub fn epoch(&self) -> Epoch {
        self.epoch
    }
}
