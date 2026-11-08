/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod lease_body_builder;

/// A partition leases a block of sequences of one type (`PROTOCOL.md` §13.5): the half-open range `start` to `end` of
/// `size` sequences. The requester sends `start` and `end` 0.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `type` | u16: the type prefix, at least 1 |
/// | 4 | `partition` | u32, at least 1 |
/// | 8 | `size` | u64: the sequences asked for, at least 1 |
/// | 16 | `start` | u64: the block's first sequence; 0 in a request |
/// | 24 | `end` | u64: `start + size`; 0 in a request |
///
/// No sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Lease {
    type_prefix: u16,
    partition: u32,
    size: u64,
    start: u64,
    end: u64,
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<Lease>() <= 64);

impl Lease {
    /// `partition`'s request for `size` sequences of the type `type_prefix`.
    #[must_use]
    pub fn request(type_prefix: u16, partition: u32, size: u64) -> Self {
        Self {
            type_prefix,
            partition,
            size,
            start: 0,
            end: 0,
        }
    }

    /// The reply: the block from `start`, ending at `start + size`, saturating.
    #[must_use]
    pub fn with_block(mut self, start: u64) -> Self {
        self.start = start;
        self.end = start.saturating_add(self.size);
        self
    }

    /// The type prefix.
    #[must_use]
    pub fn type_prefix(&self) -> u16 {
        self.type_prefix
    }

    /// The leasing partition.
    #[must_use]
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// The sequences asked for.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.size
    }

    /// The block's first sequence; 0 in a request.
    #[must_use]
    pub fn start(&self) -> u64 {
        self.start
    }

    /// One past the block's last sequence; 0 in a request.
    #[must_use]
    pub fn end(&self) -> u64 {
        self.end
    }
}
