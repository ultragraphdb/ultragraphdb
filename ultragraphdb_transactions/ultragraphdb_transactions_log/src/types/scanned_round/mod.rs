/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU32;

use ultragraphdb_protocol_wire::RoundHeader;
use ultragraphdb_traits_log::Position;

use crate::utils::block::padded;

/// A round a scan accepted: its header's fields, its frame's `checksum`, and where its frame lies in the segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScannedRound {
    position: Position,
    parent: u64,
    checksum: u64,
    first_version: u64,
    transactions: u32,
    derivation: NonZeroU32,
    offset: u64,
    size: u32,
}

impl ScannedRound {
    /// The round whose command header reads `header`, with its frame's `checksum`, its frame of `size` bytes at
    /// `offset`.
    pub fn new(header: RoundHeader, checksum: u64, offset: u64, size: u32) -> Self {
        Self {
            position: Position::new(header.position()),
            parent: header.parent(),
            checksum,
            first_version: header.first_version(),
            transactions: header.transactions(),
            derivation: header.derivation(),
            offset,
            size,
        }
    }

    /// The round's position, as its header names it.
    pub fn position(&self) -> Position {
        self.position
    }

    /// The checksum of the round before it, as its header names it.
    pub fn parent(&self) -> u64 {
        self.parent
    }

    /// The frame's `checksum`, the parent the next round names.
    pub fn checksum(&self) -> u64 {
        self.checksum
    }

    /// The commit version of the round's first transaction.
    pub fn first_version(&self) -> u64 {
        self.first_version
    }

    /// The number of decisions the round carries, one per entry.
    pub fn transactions(&self) -> u32 {
        self.transactions
    }

    /// The version of the derivation that turns the round's quads into keys.
    pub fn derivation(&self) -> NonZeroU32 {
        self.derivation
    }

    /// The frame's first byte within the segment, a multiple of 4,096.
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// The frame's `size`, header and body.
    pub fn size(&self) -> u32 {
        self.size
    }

    /// The bytes the round takes in the segment: its frame padded to whole 4,096-byte blocks.
    pub fn padded_size(&self) -> u64 {
        padded(u64::from(self.size))
    }
}
