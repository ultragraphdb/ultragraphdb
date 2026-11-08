/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// An entry of a key-value section: a key and its value, `None` for a removal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordEntry<'a> {
    key: &'a [u8],
    value: Option<&'a [u8]>,
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<RecordEntry<'static>>() <= 64);

impl<'a> RecordEntry<'a> {
    /// The entry of `key` and `value`, `None` for a removal.
    pub fn new(key: &'a [u8], value: Option<&'a [u8]>) -> Self {
        Self { key, value }
    }

    /// The key.
    pub fn key(&self) -> &'a [u8] {
        self.key
    }

    /// The value, `None` when the entry removes its key.
    pub fn value(&self) -> Option<&'a [u8]> {
        self.value
    }
}
