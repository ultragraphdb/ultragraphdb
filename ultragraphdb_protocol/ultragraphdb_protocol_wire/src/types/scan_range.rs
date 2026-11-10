/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::as_u32;

use crate::utils::constants::SCAN_RANGE_SIZE;

/// One range of a Scan (`PROTOCOL.md` §13.13): the partition read, the first key, inclusive, the key past the range's
/// last, exclusive, empty for the end of the first key's tenant in its keyspace, and the object encoding its first list
/// resumes at, empty for none. Keys are the store's keys without their version suffix. A Scan's builder copies the keys;
/// its view borrows them from the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScanRange<'a> {
    partition: u32,
    first: &'a [u8],
    past: &'a [u8],
    after: &'a [u8],
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<ScanRange<'static>>() <= 64);

impl<'a> ScanRange<'a> {
    /// The range of `partition` from `first` to `past`, resuming at no object.
    #[must_use]
    pub fn new(partition: u32, first: &'a [u8], past: &'a [u8]) -> Self {
        Self {
            partition,
            first,
            past,
            after: &[],
        }
    }

    /// The range resuming its first list at the object encoding `after`.
    #[must_use]
    pub fn with_after(mut self, after: &'a [u8]) -> Self {
        self.after = after;
        self
    }

    /// The partition read, the one the role address serves.
    #[must_use]
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// The range's first key, inclusive.
    #[must_use]
    pub fn first(&self) -> &'a [u8] {
        self.first
    }

    /// The key past the range's last, exclusive; empty for the end of the first key's tenant in its keyspace.
    #[must_use]
    pub fn past(&self) -> &'a [u8] {
        self.past
    }

    /// The object encoding the range's first list resumes at, that object included when the list holds it; empty for
    /// none.
    #[must_use]
    pub fn after(&self) -> &'a [u8] {
        self.after
    }

    /// The range's 24-byte record, its first key at offset `at` of the keys' section, its past key and its `after` right
    /// after: `partition` u32, `after_len` u32, `first` u32, `first_len` u32, `past` u32, `past_len` u32.
    #[must_use]
    pub fn encode(&self, at: u32) -> [u8; SCAN_RANGE_SIZE] {
        let first_len = as_u32(self.first.len());
        let fields = [
            self.partition,
            as_u32(self.after.len()),
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
