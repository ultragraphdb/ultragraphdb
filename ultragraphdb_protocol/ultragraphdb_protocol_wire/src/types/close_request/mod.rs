/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod close_request_body_builder;

/// A partition whose open epoch's sequence would run out asks the sequencer to close that epoch now
/// (`PROTOCOL.md` §13.16), and so does an endpoint core of compute for a snapshot read waiting on the open epoch;
/// the close, arriving on the requester's Subscribe stream, is the answer.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `partition` | u32: the asking partition, at least 1; 0 from compute |
/// | 8 | `epoch` | u64: the partition's open epoch, at least 1 |
///
/// No sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CloseRequest {
    partition: u32,
    epoch: u64,
}

impl CloseRequest {
    /// `partition`'s request to close its open `epoch`.
    #[must_use]
    pub fn new(partition: u32, epoch: u64) -> Self {
        Self { partition, epoch }
    }

    /// Compute's request to close the open `epoch`: `partition` 0.
    #[must_use]
    pub fn compute(epoch: u64) -> Self {
        Self {
            partition: 0,
            epoch,
        }
    }

    /// The asking partition; 0 for compute.
    #[must_use]
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// The partition's open epoch.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
}
