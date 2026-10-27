/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::le::bytes_at;

/// The command header of a Round frame, the log record: what makes a round self-identifying, so a scan of the log
/// stops at the first block that is not the next round of this log.
///
/// | Offset | Field | Type |
/// |---|---|---|
/// | 0 | `position` | u64, the round's place in the log, gapless from 1 |
/// | 8 | `parent` | u64, the previous round's frame `checksum` |
/// | 16 | `incarnation` | u64, the log's incarnation, raised each time a log is created afresh |
/// | 24 | `first_version` | u64, the commit version of the round's first transaction |
/// | 32 | `transactions` | u32, the round's transaction count |
/// | 36 | reserved | 92 bytes, zero |
///
/// Offsets are within the command header, the frame's bytes 128 to 256; every field is little-endian. The cluster
/// is the frame header's own field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RoundHeader {
    position: u64,
    parent: u64,
    incarnation: u64,
    first_version: u64,
    transactions: u32,
}

impl RoundHeader {
    pub fn new(
        position: u64,
        parent: u64,
        incarnation: u64,
        first_version: u64,
        transactions: u32,
    ) -> Self {
        Self {
            position,
            parent,
            incarnation,
            first_version,
            transactions,
        }
    }

    /// The fields read from a command header. The reserved bytes are not read: the frame's header checksum covers
    /// them, and only a writer of the log sets them.
    pub fn decode(command_header: &[u8; COMMAND_HEADER_SIZE]) -> Self {
        // Each field lies inside the 128 bytes, so every read is present; a missing one would read as zero.
        let u64_at = |at| bytes_at(command_header, at).map_or(0, u64::from_le_bytes);
        Self {
            position: u64_at(0),
            parent: u64_at(8),
            incarnation: u64_at(16),
            first_version: u64_at(24),
            transactions: bytes_at(command_header, 32).map_or(0, u32::from_le_bytes),
        }
    }

    /// The command header's 128 bytes, the reserved ones zero.
    pub fn encode(&self) -> [u8; COMMAND_HEADER_SIZE] {
        let fields: Vec<u8> = [
            &self.position.to_le_bytes()[..],
            &self.parent.to_le_bytes(),
            &self.incarnation.to_le_bytes(),
            &self.first_version.to_le_bytes(),
            &self.transactions.to_le_bytes(),
        ]
        .concat();
        let mut header = [0u8; COMMAND_HEADER_SIZE];
        header
            .iter_mut()
            .zip(fields)
            .for_each(|(out, byte)| *out = byte);
        header
    }

    pub fn position(&self) -> u64 {
        self.position
    }

    pub fn parent(&self) -> u64 {
        self.parent
    }

    pub fn incarnation(&self) -> u64 {
        self.incarnation
    }

    pub fn first_version(&self) -> u64 {
        self.first_version
    }

    pub fn transactions(&self) -> u32 {
        self.transactions
    }
}
