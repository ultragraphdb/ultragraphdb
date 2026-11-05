/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod entries_body_builder;

use crate::types::entries_end::EntriesEnd;

/// A batch of one scanned range's entries, in key order (`PROTOCOL.md` §13.14): each key once, with its value visible
/// at the version the range was read at.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `entries` | u32: the batch's entry count |
/// | 4 | `range` | u32: the range's index in its Scan |
/// | 8 | `end` | u8, an [`EntriesEnd`] |
/// | 9 | `continues` | u8: 1 the batch's last entry's value continues in the next frame, which needs a last entry; 0 otherwise, and whenever `end` is not 0 |
/// | 16 | `epoch` | u64: the epoch of the version the range was read at |
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
    epoch: u64,
    entries: Vec<(Vec<u8>, Vec<u8>)>,
}

impl Entries {
    /// A batch of range `range` read at `epoch`, ending as `end` says, with no entries yet.
    pub fn new(range: u32, end: EntriesEnd, epoch: u64) -> Self {
        Self {
            range,
            end,
            continues: false,
            epoch,
            entries: Vec::new(),
        }
    }

    /// A batch of range `range` read at `epoch` whose last entry's value continues in the next frame, with no entries
    /// yet, of which it needs one: as every builder, it writes what it is given, and the view refuses a frame holding
    /// none. More of the range follows.
    pub fn continuing(range: u32, epoch: u64) -> Self {
        Self {
            continues: true,
            ..Self::new(range, EntriesEnd::More, epoch)
        }
    }

    /// Appends the entry of `key` and `value`; entries are pushed in ascending key order.
    pub fn push(&mut self, key: &[u8], value: &[u8]) {
        self.entries.push((key.to_vec(), value.to_vec()));
    }

    /// The range's index in its Scan.
    pub fn range(&self) -> u32 {
        self.range
    }

    /// How the range ended.
    pub fn end(&self) -> EntriesEnd {
        self.end
    }

    /// Whether the batch's last entry's value continues in the next frame.
    pub fn continues(&self) -> bool {
        self.continues
    }

    /// The epoch of the version the range was read at.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The entries, each a key and its value, in key order.
    pub fn entries(&self) -> &[(Vec<u8>, Vec<u8>)] {
        &self.entries
    }
}
