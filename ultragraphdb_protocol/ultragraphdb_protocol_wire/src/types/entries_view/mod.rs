/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod entries_view_body_view;

use crate::types::entries_end::EntriesEnd;
use crate::types::key_values::KeyValues;

/// The body of a received Entries frame, read in place; see [`crate::Entries`] for its layout. Every entry's value is
/// present and the keys ascend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntriesView<'a> {
    range: u32,
    end: EntriesEnd,
    continues: bool,
    epoch: u64,
    entries: KeyValues<'a>,
}

impl<'a> EntriesView<'a> {
    /// The range's index in its Scan.
    pub fn range(&self) -> u32 {
        self.range
    }

    /// How the range ended.
    pub fn end(&self) -> EntriesEnd {
        self.end
    }

    /// Whether the batch's last entry's value continues in the next frame; only with [`EntriesEnd::More`] and an entry.
    pub fn continues(&self) -> bool {
        self.continues
    }

    /// The epoch of the version the range was read at.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The entries, in key order, every value present.
    pub fn entries(&self) -> KeyValues<'a> {
        self.entries
    }
}
