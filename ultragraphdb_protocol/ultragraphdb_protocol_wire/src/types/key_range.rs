/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::utils::constants::DICTIONARY_KEYSPACE;

/// A range of the Scan, its keys borrowed from the sequence's buffer, as the rules placing a frame's entries read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct KeyRange<'k> {
    pub(crate) first: &'k [u8],
    pub(crate) past: &'k [u8],
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<KeyRange<'static>>() <= 64);

impl KeyRange<'_> {
    /// Whether the range is a one-key range of the dictionary: its past key its first key followed by a zero byte.
    pub(crate) fn is_dictionary_key(&self) -> bool {
        self.first.first() == Some(&DICTIONARY_KEYSPACE)
            && self.past.split_last() == Some((&0, self.first))
    }

    /// Whether `key` lies in the range.
    pub(crate) fn holds(&self, key: &[u8]) -> bool {
        key >= self.first && (self.past.is_empty() || key < self.past)
    }
}
