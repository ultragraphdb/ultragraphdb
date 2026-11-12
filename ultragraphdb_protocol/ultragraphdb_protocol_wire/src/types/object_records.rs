/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::command::Command;
use crate::types::object_record::{ObjectRecord, STATEMENT_ID_SIZE};
use crate::utils::le::bytes_at;

/// The record at `at` of `value` and the offset after it; `None` when it runs past.
fn record_at(value: &[u8], at: usize) -> Option<(ObjectRecord<'_>, usize)> {
    let encoding_len = u32::from_le_bytes(bytes_at(value, at)?) as usize; // lossless: a `usize` is at least 32 bits
    // Lossless: a `usize` is at least 32 bits.
    let statements = u32::from_le_bytes(bytes_at(value, at.checked_add(4)?)?) as usize;
    let start = at.checked_add(8)?;
    let end = start.checked_add(encoding_len)?;
    let next = end.checked_add(statements.checked_mul(STATEMENT_ID_SIZE)?)?;
    Some((
        ObjectRecord::new(value.get(start..end)?, value.get(end..next)?),
        next,
    ))
}

/// A value of a keyspace of lists read in place: the list's objects visible at the version, in the list's order, each
/// an object record (`PROTOCOL.md` §13.14). The records follow each other with no gap, and a frame with `continues` 1
/// ends between two of them, so each frame's value reads alone.
///
/// | Object record offset | Field | Type |
/// |---|---|---|
/// | 0 | `encoding_len` | u32: the object's encoding's length |
/// | 4 | `statements` | u32: its statement IDs' count |
/// | 8 | `encoding` | the object as the store's keys encode it, a system ID for an IRI or blank node |
/// | | statement IDs | 16 bytes each: the IDs the object is held under, as they are |
///
/// A keyspace of lists alone holds these, and an entry's key tells its keyspace, so the receiver of a Scan's list ranges
/// applies this to [`crate::RecordEntry::value`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectRecords<'a> {
    value: &'a [u8],
    len: usize,
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<ObjectRecords<'static>>() <= 64);

impl<'a> ObjectRecords<'a> {
    /// The records of `value`, checked once: each record's lengths and bytes inside the value.
    ///
    /// # Errors
    ///
    /// [`ProtocolError::entry_out_of_bounds`], of an Entries body's section 0, naming the record and its offset in the
    /// value, for a record whose lengths or bytes reach past the value.
    pub fn read(value: &'a [u8]) -> Result<Self, ProtocolError> {
        let mut at = 0usize;
        let mut len = 0usize;
        // Each record takes at least its 8 bytes of lengths, so `at` grows by 8 or more a turn and the loop ends within
        // `value.len() / 8` turns.
        while at < value.len() {
            let (_, next) = record_at(value, at).ok_or_else(|| {
                ProtocolError::entry_out_of_bounds(Command::Entries.number(), 0, len, at)
            })?;
            len = len.checked_add(1).ok_or_else(|| {
                ProtocolError::entry_out_of_bounds(Command::Entries.number(), 0, len, at)
            })?;
            at = next;
        }
        Ok(Self { value, len })
    }

    /// The number of records.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the value holds no record.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The records in order.
    pub fn iter(&self) -> impl Iterator<Item = ObjectRecord<'a>> + 'a {
        let value = self.value;
        let mut at = 0usize;
        // The checked value holds exactly `len` records, which bounds the walk.
        std::iter::from_fn(move || {
            let (record, next) = record_at(value, at)?;
            at = next;
            Some(record)
        })
        .take(self.len)
    }
}
