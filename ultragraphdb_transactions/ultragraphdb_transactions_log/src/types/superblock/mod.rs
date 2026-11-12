/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

// Arithmetic on decoded log input is checked: an overflow here would be a malformed input
// accepted with a wrapped value, so it is refused instead (user decision 2026-11-12; the lint is not in the
// workspace table because elsewhere it flags provably bounded index math).
#![deny(clippy::arithmetic_side_effects)]

mod superblock_io;

use ultragraphdb_protocol_wire::xxh3_64;
use ultragraphdb_traits_log::Position;

/// The bytes `UGWL`, little-endian.
const MAGIC: u32 = 0x4C57_4755;

/// The record's format.
const VERSION: u16 = 3;

/// The checksum covers the bytes before it.
const CHECKSUMMED: usize = 56;

// The checksum is the record's last eight bytes.
const _: () = assert!(CHECKSUMMED + 8 == Superblock::SIZE);

/// The write-ahead log's superblock, one of its two copies: what the log needs that its rounds cannot say. Written
/// at the log's creation and at checkpoints, never on the acknowledgment path.
///
/// | Offset | Field | Type |
/// |---|---|---|
/// | 0 | magic | u32, `0x4C57_4755`, the bytes `UGWL` |
/// | 4 | version | u16, 3 |
/// | 6 | reserved | u16, zero |
/// | 8 | `sequence` | u64, raised by every write; the higher of two copies is the newer |
/// | 16 | `cluster` | u64 |
/// | 24 | `incarnation` | u64 |
/// | 32 | `checkpoint` | u64, the last checkpoint's position, which recovery refuses a store's applied position below |
/// | 40 | `checkpoint_checksum` | u64, that round's checksum, the parent of the round after it |
/// | 48 | `first_segment` | u64, the first segment kept |
/// | 56 | checksum | u64, XXH3-64 of bytes 0 to 56 |
///
/// Every field is little-endian. The copy a write goes to follows from its sequence: odd to `a`, even to `b`, so a
/// write never replaces the copy holding the newest record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Superblock {
    sequence: u64,
    cluster: u64,
    incarnation: u64,
    checkpoint: Position,
    checkpoint_checksum: u64,
    first_segment: u64,
}

impl Superblock {
    /// A record's length.
    pub const SIZE: usize = 64;

    /// A record of `sequence` for the log of `cluster` and `incarnation`: its last checkpoint at `checkpoint` with
    /// `checkpoint_checksum`, and its first kept segment `first_segment`.
    pub fn new(
        sequence: u64,
        cluster: u64,
        incarnation: u64,
        checkpoint: Position,
        checkpoint_checksum: u64,
        first_segment: u64,
    ) -> Self {
        Self {
            sequence,
            cluster,
            incarnation,
            checkpoint,
            checkpoint_checksum,
            first_segment,
        }
    }

    /// The record's 64 bytes.
    pub fn encode(&self) -> [u8; Self::SIZE] {
        let fields: Vec<u8> = [
            &MAGIC.to_le_bytes()[..],
            &VERSION.to_le_bytes(),
            &0u16.to_le_bytes(),
            &self.sequence.to_le_bytes(),
            &self.cluster.to_le_bytes(),
            &self.incarnation.to_le_bytes(),
            &self.checkpoint.get().to_le_bytes(),
            &self.checkpoint_checksum.to_le_bytes(),
            &self.first_segment.to_le_bytes(),
        ]
        .concat();
        debug_assert_eq!(
            fields.len(),
            CHECKSUMMED,
            "the fields fill the checksummed bytes"
        );
        let checksum = xxh3_64(&fields);
        let mut record = [0u8; Self::SIZE];
        record
            .iter_mut()
            .zip(fields.iter().chain(checksum.to_le_bytes().iter()))
            .for_each(|(out, byte)| *out = *byte);
        record
    }

    /// The record in `bytes`, `None` unless they are exactly a record this build writes: 64 bytes, the magic, version
    /// 3, the reserved bytes zero and the checksum matching.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != Self::SIZE {
            return None;
        }
        let u64_at = |at: usize| -> Option<u64> {
            Some(u64::from_le_bytes(
                bytes.get(at..at.checked_add(8)?)?.try_into().ok()?,
            ))
        };
        // The magic, the version and the zero reserved bytes are the record's first eight bytes.
        if u64_at(0)? != u64::from(MAGIC) | (u64::from(VERSION) << 32) {
            return None;
        }
        if xxh3_64(bytes.get(..CHECKSUMMED)?) != u64_at(CHECKSUMMED)? {
            return None;
        }
        Some(Self::new(
            u64_at(8)?,
            u64_at(16)?,
            u64_at(24)?,
            Position::new(u64_at(32)?),
            u64_at(40)?,
            u64_at(48)?,
        ))
    }

    /// The copy this record is written to, `'a'` or `'b'`.
    pub fn copy(&self) -> char {
        if self.sequence % 2 == 1 { 'a' } else { 'b' }
    }

    /// The record's sequence: the higher of two copies is the newer.
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// The cluster the log belongs to.
    pub fn cluster(&self) -> u64 {
        self.cluster
    }

    /// The incarnation of the log's rounds.
    pub fn incarnation(&self) -> u64 {
        self.incarnation
    }

    /// The last checkpoint's position.
    pub fn checkpoint(&self) -> Position {
        self.checkpoint
    }

    /// The checksum of the round at the checkpoint, the parent of the round after it.
    pub fn checkpoint_checksum(&self) -> u64 {
        self.checkpoint_checksum
    }

    /// The index of the first segment kept.
    pub fn first_segment(&self) -> u64 {
        self.first_segment
    }
}
