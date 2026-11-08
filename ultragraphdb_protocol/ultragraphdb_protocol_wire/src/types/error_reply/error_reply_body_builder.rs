/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_builder::BodyBuilder;
use crate::types::chunk_layout::ChunkLayout;
use crate::types::command::Command;
use crate::types::error_code::ErrorCode;
use crate::types::error_reply::ErrorReply;
use crate::types::header::Header;
use crate::types::term_table::TermTable;
use crate::utils::command_header::command_header;
use crate::utils::constants::{COMMAND_HEADER_SIZE, HEADER_SIZE, LEAF_SIZE, MAX_FRAME_FLOOR};
use crate::utils::sections::{align8, write_sections};

impl BodyBuilder for ErrorReply {
    const COMMAND: Command = Command::Error;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.code.number().to_le_bytes()),
            (8, &self.detail.to_le_bytes()),
        ])
    }

    /// The frame of the reply under `header`, within the floor of `max_frame` as [`ErrorReply::body`] leaves it; a `detail` its code's rule refuses, as the view would, is refused as
    /// [`ProtocolError::field_value`] (`"detail"`): `WrongPartition`'s bucket takes bits 0 to 23, its partition bits
    /// 32 to 63 (`PROTOCOL.md` §9.4).
    fn frame(&self, header: Header) -> Result<Vec<u8>, ProtocolError> {
        if self.code == ErrorCode::WrongPartition && self.detail & 0xFF00_0000 != 0 {
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

    /// The body: the message, cut at its last character boundary that leaves the frame within the floor of `max_frame`,
    /// which every peer takes, and the value (`PROTOCOL.md` §9.4).
    fn body(&self) -> Vec<u8> {
        let mut terms = TermTable::new();
        if let Some(value) = &self.value {
            // A value the table refuses is no value; ErrorReply::with_value states which terms a table holds.
            let _index: Option<u32> = terms.intern(value.clone()).ok();
        }
        let (records, text) = terms.sections();
        let message = fitted(&self.message, align8(records.len()) + text.len());
        write_sections(&[], &[message.as_bytes(), &records, &text])
    }
}

/// `message` cut at its last character boundary within what a frame at the floor of `max_frame` leaves it beside a table of
/// three sections and `after` bytes of sections 1 and 2; whole when it fits.
fn fitted(message: &str, after: usize) -> &str {
    let body = MAX_FRAME_FLOOR - HEADER_SIZE;
    // A chunk table of as many leaves as the whole body would take, so the data and its table stay within the body.
    let data = body - LEAF_SIZE * ChunkLayout::of_data(body).chunks();
    // Section 1 starts 8-aligned after section 0, so section 0 takes its length rounded up to 8.
    let room = data.saturating_sub(3 * 8 + after) / 8 * 8;
    message
        .get(..message.floor_char_boundary(room))
        .unwrap_or_default()
}
