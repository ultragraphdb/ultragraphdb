/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod entries_body_builder;

use crate::types::chunk_layout::ChunkLayout;
use crate::types::entries_end::EntriesEnd;
use crate::utils::constants::HEADER_SIZE;
use crate::utils::key_value_writer::entry_size;

/// The bytes of an Entries body's table of its one section, which lies right after it.
const SECTION_TABLE_SIZE: usize = 8;

/// The size of the frame of an Entries body whose section holds `section_len` bytes: the header, the section table, the
/// section and its chunk table.
fn frame_size_of(section_len: usize) -> usize {
    let data_len = SECTION_TABLE_SIZE.saturating_add(section_len);
    HEADER_SIZE.saturating_add(ChunkLayout::of_data(data_len).body_len())
}

/// A batch of one scanned range's entries, in key order, or of a run of one-key dictionary ranges', in their ranges'
/// order (`PROTOCOL.md` §13.14): each key once, with its value visible at the version the range was read at.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `entries` | u32: the batch's entry count |
/// | 4 | `range` | u32: the range's index in its Scan |
/// | 8 | `end` | u8, an [`EntriesEnd`] |
/// | 9 | `continues` | u8: 1 the batch's last entry's value continues in the next frame, which needs a last entry; 0 otherwise, and whenever `end` is not 0 |
/// | 12 | `through` | u32: the batch's last range: `range`, or above it when the batch answers each range from `range` through this one, each a one-key range of the dictionary, each ended |
/// | 16 | `epoch` | u64: the epoch of the version the range was read at |
/// | 24 | `version` | u64: the version the Scan chose, a version or version bound, the same in every frame of the Scan |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the entries, in the key-value layout of a write-set record, every value present |
///
/// A value is what the keyspace holds at the version: the stored bytes in the dictionary and the referrer sets, and in a
/// keyspace of lists the list's objects as object records ([`crate::ObjectRecords`]). A list larger than a frame
/// continues across frames: a frame with `continues` 1 ends between two records, and the next frame's first entry has
/// the same key and the next records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entries {
    range: u32,
    end: EntriesEnd,
    continues: bool,
    through: u32,
    epoch: u64,
    version: u64,
    entries: Vec<(Vec<u8>, Vec<u8>)>,
    section_len: usize,
}

impl Entries {
    /// A batch of range `range` alone read at `epoch`, ending as `end` says, of version 0 until set, with no entries yet.
    #[must_use]
    pub fn new(range: u32, end: EntriesEnd, epoch: u64) -> Self {
        Self {
            range,
            end,
            continues: false,
            through: range,
            epoch,
            version: 0,
            entries: Vec::new(),
            section_len: 0,
        }
    }

    /// A batch of range `range` read at `epoch` whose last entry's value continues in the next frame, with no entries
    /// yet, of which it needs one: as every builder, it writes what it is given, and the view refuses a frame holding
    /// none. More of the range follows.
    #[must_use]
    pub fn continuing(range: u32, epoch: u64) -> Self {
        Self {
            continues: true,
            ..Self::new(range, EntriesEnd::More, epoch)
        }
    }

    /// The batch naming `version`, the version its Scan chose.
    #[must_use]
    pub fn with_version(mut self, version: u64) -> Self {
        self.version = version;
        self
    }

    /// The batch answering every range from its `range` through `through`, each ended.
    #[must_use]
    pub fn with_through(mut self, through: u32) -> Self {
        self.through = through;
        self
    }

    /// The batch ending as `end` says.
    #[must_use]
    pub fn with_end(mut self, end: EntriesEnd) -> Self {
        self.end = end;
        self
    }

    /// The batch with its last entry's value continuing in the next frame, which needs an entry. More of the range follows.
    #[must_use]
    pub fn continued(mut self) -> Self {
        self.continues = true;
        self.end = EntriesEnd::More;
        self
    }

    /// Appends the entry of `key` and `value`; entries are pushed in ascending key order, a run's in its ranges' order.
    pub fn push(&mut self, key: &[u8], value: &[u8]) {
        self.section_len = self
            .section_len
            .saturating_add(entry_size(key.len(), value.len()));
        self.entries.push((key.to_vec(), value.to_vec()));
    }

    /// The bytes [`crate::BodyBuilder::frame`] builds of this batch, header and chunk table included.
    #[must_use]
    pub fn frame_size(&self) -> usize {
        frame_size_of(self.section_len)
    }

    /// The bytes [`crate::BodyBuilder::frame`] builds of this batch with one more entry, of a key of `key_len` bytes and
    /// a value of `value_len`.
    #[must_use]
    pub fn frame_size_with(&self, key_len: usize, value_len: usize) -> usize {
        frame_size_of(
            self.section_len
                .saturating_add(entry_size(key_len, value_len)),
        )
    }

    /// The range's index in its Scan.
    #[must_use]
    pub fn range(&self) -> u32 {
        self.range
    }

    /// The batch's last range: its `range` when the batch answers one range.
    #[must_use]
    pub fn through(&self) -> u32 {
        self.through
    }

    /// The version the Scan chose.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// How the range ended.
    #[must_use]
    pub fn end(&self) -> EntriesEnd {
        self.end
    }

    /// Whether the batch's last entry's value continues in the next frame.
    #[must_use]
    pub fn continues(&self) -> bool {
        self.continues
    }

    /// The epoch of the version the range was read at.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The entries, each a key and its value, in key order.
    #[must_use]
    pub fn entries(&self) -> &[(Vec<u8>, Vec<u8>)] {
        &self.entries
    }
}
