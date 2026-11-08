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
use crate::utils::write_frame::WriteHeader;
use crate::utils::write_sections::WriteSections;

impl<'a> BodyView<'a> for DirectWriteView<'a> {
    const COMMAND: Command = Command::DirectWrite;

    type Body = DirectWrite;

    /// Checks a frame a role endpoint received in the order [`DirectWriteView::from_client_frame`] does, `principal` holding any
    /// value.
    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        Self::read(frame, false)
    }

    fn to_body(&self) -> Result<DirectWrite, ProtocolError> {
        Ok(DirectWrite::from_parts(
            self.header.clone(),
            self.sections.terms.to_table(),
            self.sections.changes(),
        ))
    }
}

impl<'a> DirectWriteView<'a> {
    /// A DirectWrite the client endpoint received: as [`BodyView::from_frame`] reads it, with `principal` 0.
    ///
    /// # Errors
    ///
    /// [`BodyView::from_frame`]'s refusals, and [`ProtocolError::field_value`] (`"principal"`) for a `principal` other
    /// than 0, after the token and before the sections.
    pub fn from_client_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        Self::read(frame, true)
    }

    /// Checks, in `PROTOCOL.md` §6.6's order: unused bytes 64–127 zero; `request` at least 1 with a session, 0 with
    /// none; the token; `principal` 0 when `from_client`; the sections; the term table; the names, holding none; the args
    /// a whole number of u32s; section 0 not empty; the records, a declaration refused.
    fn read(frame: &FrameView<'a>, from_client: bool) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header =
            WriteHeader::read(command, expect_command(frame, Self::COMMAND)?, from_client)?;
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
}
