/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::StringList;
use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::conditional_write::ConditionalWrite;
use crate::types::conditional_write_view::ConditionalWriteView;
use crate::types::frame_view::FrameView;
use crate::utils::body::read_sections;
use crate::utils::changes::{RecordKinds, consume_records};
use crate::utils::command_header::expect_command;
use crate::utils::commit_operation_layout::{OPERATION_FIELDS_SIZE, consume_operations};
use crate::utils::fields::count;
use crate::utils::whole_records::whole_records;
use crate::utils::write_frame::WriteHeader;
use crate::utils::write_sections::WriteSections;

impl<'a> BodyView<'a> for ConditionalWriteView<'a> {
    const COMMAND: Command = Command::ConditionalWrite;

    type Body = ConditionalWrite;

    /// Checks a frame a role endpoint received in the order [`ConditionalWriteView::from_client_frame`] does, `principal` holding any
    /// value.
    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        Self::read(frame, false)
    }

    fn to_body(&self) -> Result<ConditionalWrite, ProtocolError> {
        let operations = (0..self.operation_count())
            .filter_map(|index| self.operation(index))
            .collect();
        Ok(ConditionalWrite::from_parts(
            self.header.clone(),
            self.sections.terms.to_table(),
            self.sections.changes(),
            operations,
        ))
    }
}

impl<'a> ConditionalWriteView<'a> {
    /// A ConditionalWrite the client endpoint received: as [`BodyView::from_frame`] reads it, with `principal` 0.
    ///
    /// # Errors
    ///
    /// [`BodyView::from_frame`]'s refusals, and [`ProtocolError::field_value`] (`"principal"`) for a `principal` other
    /// than 0, after the token and before the sections.
    pub fn from_client_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        Self::read(frame, true)
    }

    /// Checks, in `PROTOCOL.md` §6.7's order: unused bytes 64–127 zero; `request` at least 1 with a session, 0 with
    /// none; the token; `principal` 0 when `from_client`; the sections; the term table; the names, holding none; the args
    /// a whole number of u32s; section 6 a whole number of operations and not empty; the records, a declaration refused;
    /// the operations, which with the records consume the args exactly.
    fn read(frame: &FrameView<'a>, from_client: bool) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header =
            WriteHeader::read(command, expect_command(frame, Self::COMMAND)?, from_client)?;
        let body = read_sections::<7>(command, frame.body(), 0)?;
        let sections = WriteSections::read(command, &body)?;
        let operations = body.get(6);
        let whole = whole_records::<OPERATION_FIELDS_SIZE>(command, 6, operations)?;
        if whole.is_empty() {
            return Err(ProtocolError::count_mismatch(command, "operations", 0, 1));
        }
        let first = consume_records(
            command,
            (0, 1),
            sections.records,
            &sections.args,
            sections.terms.len(),
            &StringList::EMPTY,
            RecordKinds::Changes,
        )?;
        let starts = consume_operations(
            command,
            1,
            body.offset(6),
            operations,
            &sections.args,
            first,
            count(command, "terms", sections.terms.len())?,
        )?;
        Ok(Self {
            header,
            sections,
            operations,
            starts,
        })
    }
}
