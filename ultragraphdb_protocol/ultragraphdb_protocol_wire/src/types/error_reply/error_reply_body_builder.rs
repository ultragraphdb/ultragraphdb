/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::{align8, write_sections};
use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_builder::BodyBuilder;
use crate::types::chunk_layout::ChunkLayout;
use crate::types::command::Command;
use crate::types::error_code::ErrorCode;
use crate::types::error_reply::ErrorReply;
use crate::types::header::Header;
use crate::types::request_term::RequestTerm;
use crate::types::term_table::TermTable;
use crate::utils::command_header::command_header;
use crate::utils::constants::{
    COMMAND_HEADER_SIZE, DETAIL_VALUE_CUT, HEADER_SIZE, LEAF_SIZE, MAX_FRAME_FLOOR,
};

impl BodyBuilder for ErrorReply {
    const COMMAND: Command = Command::Error;

    /// The code, and the detail with [`DETAIL_VALUE_CUT`] set when the frame cuts the value ([`ErrorReply::value_cut`]).
    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        let detail = if self.value_cut() {
            self.detail | DETAIL_VALUE_CUT
        } else {
            self.detail
        };
        command_header(&[
            (0, &self.code.number().to_le_bytes()),
            (8, &detail.to_le_bytes()),
        ])
    }

    /// The frame of the reply under `header`, within the floor of `max_frame` as [`ErrorReply::body`] leaves it; a `detail` its code's rule refuses, as the view would, is refused as
    /// [`ProtocolError::field_value`] (`"detail"`): `WrongPartition`'s bucket takes bits 0 to 23, its partition bits
    /// 32 to 63; `OperationFailed`'s detail bit 0, [`DETAIL_VALUE_CUT`], and no other (`PROTOCOL.md` §9.4).
    fn frame(&self, header: Header) -> Result<Vec<u8>, ProtocolError> {
        if self.code == ErrorCode::WrongPartition && self.detail & 0xFF00_0000 != 0 {
            return Err(ProtocolError::field_value(
                Self::COMMAND.number(),
                "detail",
                self.detail,
            ));
        }
        if self.code == ErrorCode::OperationFailed && self.detail & !DETAIL_VALUE_CUT != 0 {
            return Err(ProtocolError::field_value(
                Self::COMMAND.number(),
                "detail",
                self.detail,
            ));
        }
        header
            .with_command(Self::COMMAND)
            .with_command_header(self.command_header())
            .encode_frame(&self.body())
    }

    /// The body within the floor of `max_frame`, which every peer takes (`PROTOCOL.md` §9.4): the value, cut by `carried` to
    /// what the frame leaves beside an empty message, then the message, cut at its last character boundary within what
    /// the value leaves.
    fn body(&self) -> Vec<u8> {
        let mut terms = TermTable::new();
        if let Some(value) = self.value.as_ref().and_then(carried) {
            // A value the table refuses is no value; ErrorReply::with_value states which terms a table holds.
            let _index: Option<u32> = terms.intern(value).ok();
        }
        let (records, text) = terms.sections();
        let message = fitted(&self.message, align8(records.len()) + text.len());
        write_sections(&[], &[message.as_bytes(), &records, &text])
    }
}

/// The bytes of body data a frame at the floor of `max_frame` holds beside its chunk table.
fn data_room() -> usize {
    let body = MAX_FRAME_FLOOR - HEADER_SIZE;
    // A chunk table of as many leaves as the whole body would take, so the data and its table stay within the body.
    body - LEAF_SIZE * ChunkLayout::of_data(body).chunks()
}

/// The bytes of text a value's one-entry term table holds in a frame at the floor of `max_frame` beside an empty message:
/// the data less a table of three sections and the entry's 24-byte record, section 2 following it with no padding.
pub(super) fn value_room() -> usize {
    data_room() - 3 * 8 - 24
}

/// The bytes `value`'s texts take in a term table's `text` section: a triple term's none.
pub(super) fn text_length(value: &RequestTerm) -> usize {
    match value {
        RequestTerm::Iri(text) | RequestTerm::BlankNode(text) => text.len(),
        RequestTerm::Literal { lexical, datatype } => {
            lexical.len() + datatype.as_ref().map_or(0, String::len)
        }
        RequestTerm::LanguageLiteral {
            lexical, language, ..
        } => lexical.len() + language.len(),
        RequestTerm::Triple { .. } => 0,
    }
}

/// `value` as an Error frame at the floor of `max_frame` carries it beside an empty message (`PROTOCOL.md` §9.4): whole when
/// its texts fit [`value_room`]; a literal with its lexical form cut at its last character boundary within the room its
/// datatype's IRI or language tag leaves, which stays whole; `None` for an IRI or a blank node past the room, which names a
/// node by its whole text, so a cut one names another, and for a literal whose datatype's IRI or language tag alone
/// passes it.
fn carried(value: &RequestTerm) -> Option<RequestTerm> {
    let room = value_room();
    if text_length(value) <= room {
        return Some(value.clone());
    }
    let cut = |lexical: &str, kept: usize| {
        let within = room.checked_sub(kept)?;
        lexical
            .get(..lexical.floor_char_boundary(within))
            .map(str::to_string)
    };
    match value {
        RequestTerm::Literal { lexical, datatype } => Some(RequestTerm::Literal {
            lexical: cut(lexical, datatype.as_ref().map_or(0, String::len))?,
            datatype: datatype.clone(),
        }),
        RequestTerm::LanguageLiteral {
            lexical,
            language,
            direction,
        } => Some(RequestTerm::LanguageLiteral {
            lexical: cut(lexical, language.len())?,
            language: language.clone(),
            direction: *direction,
        }),
        RequestTerm::Iri(_) | RequestTerm::BlankNode(_) | RequestTerm::Triple { .. } => None,
    }
}

/// `message` cut at its last character boundary within what a frame at the floor of `max_frame` leaves it beside a table of
/// three sections and `after` bytes of sections 1 and 2; whole when it fits.
fn fitted(message: &str, after: usize) -> &str {
    let data = data_room();
    // Section 1 starts 8-aligned after section 0, so section 0 takes its length rounded up to 8.
    let room = data.saturating_sub(3 * 8 + after) / 8 * 8;
    message
        .get(..message.floor_char_boundary(room))
        .unwrap_or_default()
}
