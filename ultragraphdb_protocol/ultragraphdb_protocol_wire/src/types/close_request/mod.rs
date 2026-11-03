/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod close_request_body_builder;

/// A partition whose open epoch's sequence would run out asks the sequencer to close that epoch now
/// (`PROTOCOL.md` §13.16); the close, arriving on the partition's Subscribe stream, is the answer.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `partition` | u32: the asking partition, at least 1 |
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
    pub fn new(partition: u32, epoch: u64) -> Self {
        Self { partition, epoch }
    }

    /// The asking partition.
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// The partition's open epoch.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
}
