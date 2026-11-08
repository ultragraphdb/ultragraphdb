/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod entries_view_body_view;

use crate::types::entries_end::EntriesEnd;
use crate::types::key_values::KeyValues;

/// The body of a received Entries frame, read in place; see [`crate::Entries`] for its layout. Every entry's value is
/// present, and in a batch of one range the keys ascend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntriesView<'a> {
    range: u32,
    end: EntriesEnd,
    continues: bool,
    through: u32,
    epoch: u64,
    version: u64,
    entries: KeyValues<'a>,
}

impl<'a> EntriesView<'a> {
    /// The range's index in its Scan.
    #[must_use]
    pub fn range(&self) -> u32 {
        self.range
    }

    /// The batch's last range: [`EntriesView::range`], or above it when the batch answers each range from that one
    /// through this one, each a one-key range of the dictionary, each ended.
    #[must_use]
    pub fn through(&self) -> u32 {
        self.through
    }

    /// The version the Scan chose, a version or version bound.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// How the range ended.
    #[must_use]
    pub fn end(&self) -> EntriesEnd {
        self.end
    }

    /// Whether the batch's last entry's value continues in the next frame; only with [`EntriesEnd::More`] and an entry.
    #[must_use]
    pub fn continues(&self) -> bool {
        self.continues
    }

    /// The epoch of the version the range was read at.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The entries, every value present: in key order in a batch of one range, in the ranges' order in a run.
    #[must_use]
    pub fn entries(&self) -> KeyValues<'a> {
        self.entries
    }
}
