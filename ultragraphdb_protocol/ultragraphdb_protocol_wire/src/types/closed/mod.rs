/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod closed_body_builder;

use crate::types::closed_partition::ClosedPartition;

/// Compute asks a storage process for the latest closed epoch of each partition a monotonic-fast read reaches there
/// (`PROTOCOL.md` §13.17); the process answers with every partition as sent and its epoch filled. The command header is
/// all zero.
///
/// | Section | Contents |
/// |---|---|
/// | 0 | partitions, 16 bytes each ([`ClosedPartition`]), ascending, at least one |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closed {
    partitions: Vec<ClosedPartition>,
}

impl Closed {
    /// The body of `partitions`, ascending: the request with every epoch 0, or the reply with them filled.
    pub fn new(partitions: Vec<ClosedPartition>) -> Self {
        Self { partitions }
    }

    /// The request for `partitions`, ascending, every epoch 0.
    pub fn request(partitions: &[u32]) -> Self {
        Self::new(
            partitions
                .iter()
                .map(|partition| ClosedPartition::new(*partition, 0))
                .collect(),
        )
    }

    /// The partitions, in order.
    pub fn partitions(&self) -> &[ClosedPartition] {
        &self.partitions
    }
}
