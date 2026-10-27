/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::execute::Execute;
use crate::types::execute_view::ExecuteView;
use crate::types::frame_view::FrameView;
use crate::types::string_list::StringList;
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;

impl<'a> BodyView<'a> for ExecuteView<'a> {
    const COMMAND: Command = Command::Execute;

    type Body = Execute;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 12)])?;
        let sections = Sections::<3>::read(command, frame.body(), 0)?;
        Ok(Self {
            statement: header.u64_at(0),
            limit: header.u32_at(8),
            parameters: StringList::read(command, (0, 1), sections.get(0), sections.get(1))?,
            resume: sections.get(2),
        })
    }

    fn to_body(&self) -> Result<Execute, ProtocolError> {
        Ok(Execute::new(
            self.statement,
            self.limit,
            self.parameters.iter().map(str::to_string).collect(),
            self.resume.to_vec(),
        ))
    }
}
