/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::{StringList, StringWriter, as_u32};
use ultragraphdb_error_protocol::ProtocolError;

use crate::types::warning::Warning;
use crate::types::warning_view::WarningView;
use crate::utils::body::{read_string_list, records};
use crate::utils::constants::{WARNING_MESSAGE_MAX, WARNING_SIZE, WARNINGS_MAX};
use crate::utils::le::bytes_at;

/// The languages a diagnostic names (`PROTOCOL.md` §9.4): 1 GQL, 2 SPARQL, 3 UQL, 4 UGS.
const LANGUAGES: std::ops::RangeInclusive<u32> = 1..=4;

/// The warnings of a received reply, read in place from their three sections (`PROTOCOL.md` §4.9): the warnings, 24
/// bytes each, then their messages as a string list. Checked once, when the reply's view is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Warnings<'a> {
    records: &'a [u8],
    messages: StringList<'a>,
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<Warnings<'static>>() <= 64);

impl<'a> Warnings<'a> {
    /// The warnings of a reply of `command`, sections `sections` of its body (the warnings, then their messages' ends
    /// and text, the warnings at body offset `offset`), checked in `PROTOCOL.md` §4.9's order: whole 24-byte warnings
    /// ([`ProtocolError::section_length`]); the messages' string list (§4.3); at most [`WARNINGS_MAX`]
    /// ([`ProtocolError::count_mismatch`] `"warnings"`); then each warning: bytes 20 to 23 zero
    /// ([`ProtocolError::padding_not_zero`], the body offset); `diagnostic`'s bits 24 to 31 zero and its language 1 to 4
    /// ([`ProtocolError::field_value`] `"diagnostic"`); `source` 0 unless `sourced`, an Install's reply
    /// (`"source"`); `offset + length` within a u32 (`"location"`); `message` below the messages' count
    /// ([`ProtocolError::index_out_of_range`]); the message at most [`WARNING_MESSAGE_MAX`] bytes (`"message"`).
    pub(crate) fn read(
        command: u16,
        sections: (usize, usize, usize),
        offset: usize,
        bytes: (&'a [u8], &'a [u8], &'a [u8]),
        sourced: bool,
    ) -> Result<Self, ProtocolError> {
        let whole = records(command, sections.0, bytes.0, WARNING_SIZE)?;
        let messages = read_string_list(command, (sections.1, sections.2), bytes.1, bytes.2)?;
        let count = whole.len();
        if count > WARNINGS_MAX {
            // Lossless: a `usize` is at most 64 bits.
            return Err(ProtocolError::count_mismatch(
                command,
                "warnings",
                count as u64,
                WARNINGS_MAX as u64,
            ));
        }
        for (at, record) in whole.enumerate() {
            let start = offset.saturating_add(WARNING_SIZE.saturating_mul(at));
            check_warning(command, (sections.0, start, at), record, &messages, sourced)?;
        }
        Ok(Self {
            records: bytes.0,
            messages,
        })
    }

    /// The number of warnings.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len() / WARNING_SIZE
    }

    /// Whether the reply carries no warning.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Warning `index`, `None` past the last.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<WarningView<'a>> {
        let record = self
            .records
            .get(index.checked_mul(WARNING_SIZE)?..)?
            .get(..WARNING_SIZE)?;
        let message = bytes_at(record, 16).map(u32::from_le_bytes)?;
        let message = self.messages.get(message as usize)?; // lossless: a `usize` is at least 32 bits
        Some(WarningView::read(record, message))
    }

    /// Whether any of the three sections holds a byte: a warning or a message.
    pub(crate) fn holds_sections(&self) -> bool {
        !self.records.is_empty() || !self.messages.is_empty()
    }

    /// The warnings as their sender holds them.
    pub(crate) fn to_warnings(self) -> Vec<Warning> {
        self.iter()
            .map(|view| {
                Warning::new(view.diagnostic(), view.message())
                    .with_source(view.source())
                    .with_location(view.offset()..view.offset().saturating_add(view.length()))
            })
            .collect()
    }

    /// The warnings in order.
    pub fn iter(&self) -> impl Iterator<Item = WarningView<'a>> + 'a {
        let warnings = *self;
        (0..warnings.len()).filter_map(move |index| warnings.get(index))
    }
}

/// Warning `at`'s 24 bytes `record` at body offset `start` of section `section`, checked in §4.9's order against the
/// `messages`; its `source` nonzero only when `sourced`.
fn check_warning(
    command: u16,
    (section, start, at): (usize, usize, usize),
    record: &[u8],
    messages: &StringList<'_>,
    sourced: bool,
) -> Result<(), ProtocolError> {
    if let Some(j) = record
        .get(20..WARNING_SIZE)
        .and_then(|zero| zero.iter().position(|b| *b != 0))
    {
        return Err(ProtocolError::padding_not_zero(command, start + 20 + j));
    }
    let field = |at: usize| bytes_at(record, at).map_or(0, u32::from_le_bytes);
    let diagnostic = field(0);
    if diagnostic >> 24 != 0 || !LANGUAGES.contains(&((diagnostic >> 16) & 0xFF)) {
        return Err(ProtocolError::field_value(
            command,
            "diagnostic",
            diagnostic.into(),
        ));
    }
    let source = field(4);
    if source != 0 && !sourced {
        return Err(ProtocolError::field_value(command, "source", source.into()));
    }
    let end = u64::from(field(8)) + u64::from(field(12));
    if end > u64::from(u32::MAX) {
        return Err(ProtocolError::field_value(command, "location", end));
    }
    let message = field(16);
    let Some(text) = messages.get(message as usize) else {
        // Lossless: a `usize` is at least 32 bits.
        return Err(ProtocolError::index_out_of_range(
            command,
            section,
            at,
            message,
            as_u32(messages.len()),
        ));
    };
    if text.len() > WARNING_MESSAGE_MAX {
        // Lossless: a `usize` is at most 64 bits.
        return Err(ProtocolError::field_value(
            command,
            "message",
            text.len() as u64,
        ));
    }
    Ok(())
}

/// The three sections of `warnings` (`PROTOCOL.md` §4.9): the warnings, 24 bytes each, then their messages as a string
/// list, each distinct message once (§4.3).
pub(crate) fn warning_sections(warnings: &[Warning]) -> [Vec<u8>; 3] {
    let mut messages = StringWriter::default();
    let mut records = Vec::with_capacity(WARNING_SIZE * warnings.len());
    for warning in warnings {
        let location = warning.location();
        let length = location.end.saturating_sub(location.start);
        let message = messages.intern(warning.message());
        for field in [
            warning.diagnostic(),
            warning.source(),
            location.start,
            length,
            message,
            0,
        ] {
            records.extend_from_slice(&field.to_le_bytes());
        }
    }
    let (ends, text) = messages.sections();
    [records, ends, text]
}
