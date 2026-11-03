/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::utils::le::bytes_at;

/// A partition of a Closed body (`PROTOCOL.md` §13.17), 16 bytes: `partition` u32, 4 zero bytes, `epoch` u64, the
/// partition's latest closed epoch in a reply and 0 in a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClosedPartition {
    partition: u32,
    epoch: u64,
}

impl ClosedPartition {
    /// `partition` at its latest closed `epoch`; 0 in a request and for none.
    pub fn new(partition: u32, epoch: u64) -> Self {
        Self { partition, epoch }
    }

    /// The partition.
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// Its latest closed epoch; 0 in a request and for none.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The record's 16 bytes.
    pub fn encode(&self) -> [u8; 16] {
        let mut out = [0u8; 16];
        out.iter_mut()
            .zip(
                self.partition
                    .to_le_bytes()
                    .into_iter()
                    .chain([0; 4])
                    .chain(self.epoch.to_le_bytes()),
            )
            .for_each(|(byte, field)| *byte = field);
        out
    }

    /// The record `bytes` hold, its zero bytes unchecked.
    pub(crate) fn decode(bytes: &[u8; 16]) -> Self {
        Self {
            partition: bytes_at(bytes, 0).map_or(0, u32::from_le_bytes),
            epoch: bytes_at(bytes, 8).map_or(0, u64::from_le_bytes),
        }
    }
}
