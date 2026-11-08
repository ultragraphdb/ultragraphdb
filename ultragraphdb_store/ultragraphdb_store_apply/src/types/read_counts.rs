/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a write's reads come to so far, in what a read set holds: asked as a scan goes, so a span the read set has no room
/// for is refused before any of it leaves.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReadCounts {
    key_bytes: usize,
    lists: usize,
    statements: usize,
    ranges: usize,
    absent: usize,
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<ReadCounts>() <= 64);

impl ReadCounts {
    /// The counts of `key_bytes` bytes of keys, `lists` lists read whole, `statements` tested, `ranges` scanned and `absent`
    /// terms.
    #[must_use]
    pub fn new(
        key_bytes: usize,
        lists: usize,
        statements: usize,
        ranges: usize,
        absent: usize,
    ) -> Self {
        Self {
            key_bytes,
            lists,
            statements,
            ranges,
            absent,
        }
    }

    /// The bytes of the keys of its lists, statements and ranges.
    #[must_use]
    pub fn key_bytes(&self) -> usize {
        self.key_bytes
    }

    /// The lists read whole.
    #[must_use]
    pub fn lists(&self) -> usize {
        self.lists
    }

    /// The statements tested.
    #[must_use]
    pub fn statements(&self) -> usize {
        self.statements
    }

    /// The ranges scanned.
    #[must_use]
    pub fn ranges(&self) -> usize {
        self.ranges
    }

    /// The terms found absent.
    #[must_use]
    pub fn absent(&self) -> usize {
        self.absent
    }
}
