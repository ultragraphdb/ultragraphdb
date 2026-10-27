/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::string_list::StringList;
use crate::types::term_table_view::TermTableView;
use crate::types::write::Write;
use crate::types::write_view::WriteView;
use crate::utils::changes::check_records;
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;
use crate::utils::sections::u32s;

impl<'a> BodyView<'a> for WriteView<'a> {
    const COMMAND: Command = Command::Write;

    type Body = Write;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 8)])?;
        let sections = Sections::<6>::read(command, frame.body(), 0)?;
        let terms = TermTableView::read(
            command,
            (2, 3),
            sections.offset(2),
            sections.get(2),
            sections.get(3),
        )?;
        let names = StringList::read(command, (4, 5), sections.get(4), sections.get(5))?;
        let args = u32s(command, 1, sections.get(1))?;
        check_records(
            command,
            (0, 1),
            sections.get(0),
            &args,
            terms.len(),
            &names,
            false,
        )?;
        Ok(Self {
            namespace: header.u64_at(0),
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
        Ok(Write::new(self.namespace, self.terms.to_table(), changes))
    }
}
