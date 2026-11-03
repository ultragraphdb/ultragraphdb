/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::utils::constants::SCAN_RANGE_SIZE;
use crate::utils::sections::as_u32;

/// One range of a Scan (`PROTOCOL.md` §13.13): the partition read, the first key, inclusive, and the key past the
/// range's last, exclusive, empty for the end of the first key's keyspace. Keys are the store's keys without their
/// version suffix. A Scan's builder copies the keys; its view borrows them from the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScanRange<'a> {
    partition: u32,
    first: &'a [u8],
    past: &'a [u8],
}

impl<'a> ScanRange<'a> {
    /// The range of `partition` from `first` to `past`.
    pub fn new(partition: u32, first: &'a [u8], past: &'a [u8]) -> Self {
        Self {
            partition,
            first,
            past,
        }
    }

    /// The partition read, one the storage process hosts.
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// The range's first key, inclusive.
    pub fn first(&self) -> &'a [u8] {
        self.first
    }

    /// The key past the range's last, exclusive; empty for the end of the first key's keyspace.
    pub fn past(&self) -> &'a [u8] {
        self.past
    }

    /// The range's 24-byte record, its first key at offset `at` of the keys' section and its past key right after:
    /// `partition` u32, 4 zero bytes, `first` u32, `first_len` u32, `past` u32, `past_len` u32.
    pub fn encode(&self, at: u32) -> [u8; SCAN_RANGE_SIZE] {
        let first_len = as_u32(self.first.len());
        let fields = [
            self.partition,
            0,
            at,
            first_len,
            at.saturating_add(first_len),
            as_u32(self.past.len()),
        ];
        let mut out = [0u8; SCAN_RANGE_SIZE];
        out.iter_mut()
            .zip(fields.iter().flat_map(|field| field.to_le_bytes()))
            .for_each(|(byte, field)| *byte = field);
        out
    }
}
