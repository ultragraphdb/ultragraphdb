/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::record_entry::RecordEntry;
use crate::utils::constants::ABSENT;
use crate::utils::le::bytes_at;
use crate::utils::sections::align8;

/// The entry at `at` of `bytes`: its key, value and the offset after its padding; `None` when it runs past.
fn entry_at(bytes: &[u8], at: usize) -> Option<(RecordEntry<'_>, usize, usize)> {
    let key_len = u32::from_le_bytes(bytes_at(bytes, at)?) as usize;
    let value_len = u32::from_le_bytes(bytes_at(bytes, at.checked_add(4)?)?);
    let key_start = at.checked_add(8)?;
    let key_end = key_start.checked_add(key_len)?;
    let key = bytes.get(key_start..key_end)?;
    let (value, end) = if value_len == ABSENT {
        (None, key_end)
    } else {
        let end = key_end.checked_add(value_len as usize)?;
        (Some(bytes.get(key_end..end)?), end)
    };
    let next = align8(end);
    (next <= bytes.len()).then_some((RecordEntry::new(key, value), end, next))
}

/// A key-value section read in place: entries of `key_len` u32, `value_len` u32, the key, the value, then zeros to a
/// multiple of 8 bytes. A `value_len` of `u32::MAX` is a removal and holds no value bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyValues<'a> {
    bytes: &'a [u8],
    len: usize,
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<KeyValues<'static>>() <= 64);

impl<'a> KeyValues<'a> {
    /// The entries of `bytes`, section `section` of a body of `command` starting at `base` within it, checked once:
    /// each entry's lengths and bytes inside the section ([`ProtocolError::entry_out_of_bounds`]), its padding zero
    /// ([`ProtocolError::padding_not_zero`], at its offset from `base`).
    pub(crate) fn read(
        command: u16,
        section: usize,
        base: usize,
        bytes: &'a [u8],
    ) -> Result<Self, ProtocolError> {
        let mut at = 0usize;
        let mut len = 0usize;
        // Each entry takes at least its 8 bytes of lengths, so `at` grows by 8 or more a turn and the loop ends within
        // `bytes.len() / 8` turns.
        while at < bytes.len() {
            let (_, end, next) = entry_at(bytes, at)
                .ok_or_else(|| ProtocolError::entry_out_of_bounds(command, section, len, at))?;
            if let Some(i) = bytes
                .get(end..next)
                .and_then(|padding| padding.iter().position(|b| *b != 0))
            {
                return Err(ProtocolError::padding_not_zero(command, base + end + i));
            }
            at = next;
            len += 1;
        }
        Ok(Self { bytes, len })
    }

    /// The number of entries.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the section holds no entry.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The entries in order.
    pub fn iter(&self) -> impl Iterator<Item = RecordEntry<'a>> + 'a {
        let bytes = self.bytes;
        let mut at = 0usize;
        // The checked section holds exactly `len` entries, which bounds the walk.
        std::iter::from_fn(move || {
            let (entry, _, next) = entry_at(bytes, at)?;
            at = next;
            Some(entry)
        })
        .take(self.len)
    }
}
