/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::error_code::ErrorCode;
use crate::types::error_reply::ErrorReply;
use crate::types::header::Header;
use crate::types::term_table::TermTable;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::write_sections;

impl BodyBuilder for ErrorReply {
    const COMMAND: Command = Command::Error;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.code.number().to_le_bytes()),
            (8, &self.detail.to_le_bytes()),
        ])
    }

    /// The frame of the reply under `header`; a `detail` its code's rule refuses, as the view would, is refused as
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

    fn body(&self) -> Vec<u8> {
        let mut terms = TermTable::new();
        if let Some(value) = &self.value {
            // A value the table refuses is no value; ErrorReply::with_value states which terms a table holds.
            let _index: Option<u32> = terms.intern(value.clone()).ok();
        }
        let (records, text) = terms.sections();
        write_sections(&[], &[self.message.as_bytes(), &records, &text])
    }
}
