/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::direct_write::DirectWrite;
use crate::types::direct_write_view::DirectWriteView;
use crate::types::frame_view::FrameView;
use crate::types::string_list::StringList;
use crate::utils::changes::{RecordKinds, check_records};
use crate::utils::sections::{Sections, expect_command};
use crate::utils::write_frame::{WriteHeader, WriteSections};

impl<'a> BodyView<'a> for DirectWriteView<'a> {
    const COMMAND: Command = Command::DirectWrite;

    type Body = DirectWrite;

    /// Checks, in `PROTOCOL.md` §6.6's order: unused bytes 64–127 zero; `request` at least 1 with a session, 0 with
    /// none; the token; the
    /// sections; the term table; the names, holding none; the args a whole number of u32s; section 0 not empty; the records, a
    /// declaration refused.
    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = WriteHeader::read(command, expect_command(frame, Self::COMMAND)?)?;
        let body = Sections::<6>::read(command, frame.body(), 0)?;
        let sections = WriteSections::read(command, &body)?;
        if sections.records.is_empty() {
            return Err(ProtocolError::count_mismatch(command, "changes", 0, 1));
        }
        check_records(
            command,
            (0, 1),
            sections.records,
            &sections.args,
            sections.terms.len(),
            &StringList::EMPTY,
            RecordKinds::Changes,
        )?;
        Ok(Self { header, sections })
    }

    fn to_body(&self) -> Result<DirectWrite, ProtocolError> {
        Ok(DirectWrite::from_parts(
            self.header.clone(),
            self.sections.terms.to_table(),
            self.sections.changes(),
        ))
    }
}
