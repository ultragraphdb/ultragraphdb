/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::{as_u32, write_sections};
use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::defaults::Defaults;
use crate::types::defaults_view::DefaultsView;
use crate::types::header::Header;
use crate::types::term_table::kind;
use crate::utils::checked_frame::checked_frame;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;

/// The size of a term table's record (`PROTOCOL.md` §4.4).
const TERM_RECORD_SIZE: usize = 24;

/// The bytes of the section table of a Defaults body: two entries of 8 bytes (`PROTOCOL.md` §4.1).
const SECTION_TABLE_SIZE: usize = 16;

/// The body of a Defaults frame holding `predicates`: a term table of one IRI record per predicate, as given, and
/// their texts back to back (`PROTOCOL.md` §4.4). A predicate given twice is written twice, which the view refuses.
pub(super) fn table_body(predicates: &[String]) -> Vec<u8> {
    let mut records = Vec::with_capacity(TERM_RECORD_SIZE.saturating_mul(predicates.len()));
    let mut text: Vec<u8> = Vec::new();
    for predicate in predicates {
        let (first, len) = (as_u32(text.len()), as_u32(predicate.len()));
        text.extend_from_slice(predicate.as_bytes());
        records.extend_from_slice(&[kind::IRI, 0, 0, 0, 0, 0, 0, 0]);
        records.extend_from_slice(&first.to_le_bytes());
        records.extend_from_slice(&len.to_le_bytes());
        // The second text, empty, starts where the first ends.
        records.extend_from_slice(&as_u32(text.len()).to_le_bytes());
        records.extend_from_slice(&0u32.to_le_bytes());
    }
    write_sections(&[], &[&records, &text])
}

/// The length of [`table_body`] of `count` predicates whose texts take `text` bytes: the section table, the records, a
/// multiple of 8, and the text right after them.
pub(super) fn table_len(count: usize, text: usize) -> usize {
    SECTION_TABLE_SIZE
        .saturating_add(TERM_RECORD_SIZE.saturating_mul(count))
        .saturating_add(text)
}

/// The length of [`table_body`] of `predicates`.
pub(super) fn body_len(predicates: &[String]) -> usize {
    let text = predicates
        .iter()
        .fold(0usize, |sum, predicate| sum.saturating_add(predicate.len()));
    table_len(predicates.len(), text)
}

impl BodyBuilder for Defaults {
    const COMMAND: Command = Command::Defaults;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.tenant().to_le_bytes()),
            (8, &self.position().to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        table_body(self.predicates())
    }

    /// The one frame of the body under `header`; a body [`crate::DefaultsView`] refuses is refused the same way, the
    /// refusals [`Defaults`] lists.
    fn frame(&self, header: Header) -> Result<Vec<u8>, ProtocolError> {
        checked_frame(
            Self::COMMAND,
            &header,
            &self.command_header(),
            &self.body(),
            |fields, body| DefaultsView::read(fields, body, 0),
        )
    }
}
