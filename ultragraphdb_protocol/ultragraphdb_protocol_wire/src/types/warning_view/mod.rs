/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::utils::le::bytes_at;

/// A warning of a received reply, read in place; see [`crate::Warning`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WarningView<'a> {
    diagnostic: u32,
    source: u32,
    offset: u32,
    length: u32,
    message: &'a str,
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<WarningView<'static>>() <= 64);

impl<'a> WarningView<'a> {
    /// The warning of `record`'s 24 bytes, its message `message`; the record's fields are unchecked.
    pub(crate) fn read(record: &[u8], message: &'a str) -> Self {
        let field = |at: usize| bytes_at(record, at).map_or(0, u32::from_le_bytes);
        Self {
            diagnostic: field(0),
            source: field(4),
            offset: field(8),
            length: field(12),
            message,
        }
    }

    /// The diagnostic: its number in bits 0–15 and its language in bits 16–23.
    #[must_use]
    pub fn diagnostic(&self) -> u32 {
        self.diagnostic
    }

    /// The diagnostic's number, its code's four digits: `UGS0902` is 902.
    #[must_use]
    pub fn number(&self) -> u16 {
        let [low, high, _, _] = self.diagnostic.to_le_bytes();
        u16::from_le_bytes([low, high])
    }

    /// The diagnostic's language (`PROTOCOL.md` §9.4): 1 GQL, 2 SPARQL, 3 UQL, 4 UGS.
    #[must_use]
    pub fn language(&self) -> u8 {
        let [_, _, language, _] = self.diagnostic.to_le_bytes();
        language
    }

    /// The source the location lies in: an Install's source's place, from 0; 0 for a query text.
    #[must_use]
    pub fn source(&self) -> u32 {
        self.source
    }

    /// The location's first byte within its source's text.
    #[must_use]
    pub fn offset(&self) -> u32 {
        self.offset
    }

    /// The location's length in bytes.
    #[must_use]
    pub fn length(&self) -> u32 {
        self.length
    }

    /// The message, for people.
    #[must_use]
    pub fn message(&self) -> &'a str {
        self.message
    }
}
