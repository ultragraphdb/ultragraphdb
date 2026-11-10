/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::term_table_view::TermTableView;
use crate::types::write::Write;
use crate::types::write_view::WriteView;
use crate::utils::body::{read_sections, read_string_list, u32s};
use crate::utils::changes::{RecordKinds, check_records};
use crate::utils::command_header::expect_command;

impl<'a> BodyView<'a> for WriteView<'a> {
    const COMMAND: Command = Command::Write;

    type Body = Write;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[])?;
        let sections = read_sections::<6>(command, frame.body(), 0)?;
        let terms = TermTableView::read(
            command,
            (2, 3),
            sections.offset(2),
            sections.get(2),
            sections.get(3),
        )?;
        let names = read_string_list(command, (4, 5), sections.get(4), sections.get(5))?;
        let args = u32s(command, 1, sections.get(1))?;
        check_records(
            command,
            (0, 1),
            sections.get(0),
            &args,
            terms.len(),
            &names,
            RecordKinds::Every,
        )?;
        Ok(Self {
            records: sections.get(0),
            args,
            terms,
            names,
        })
    }

    fn to_body(&self) -> Result<Write, ProtocolError> {
        let changes = (0..self.len())
            .filter_map(|index| self.change(index))
            .collect();
        Ok(Write::new(self.terms.to_table(), changes))
    }
}
