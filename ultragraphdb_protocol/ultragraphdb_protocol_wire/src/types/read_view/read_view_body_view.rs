/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::quad_pattern::QuadPattern;
use crate::types::read::Read;
use crate::types::read_view::ReadView;
use crate::types::string_list::StringList;
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;

impl<'a> BodyView<'a> for ReadView<'a> {
    const COMMAND: Command = Command::Read;

    type Body = Read;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 12)])?;
        let sections = Sections::<4>::read(command, frame.body(), 0)?;
        let terms = StringList::read(command, (1, 2), sections.get(1), sections.get(2))?;
        let pattern = QuadPattern::read(command, (0, 1, 2), sections.get(0), &terms, false)?;
        Ok(Self {
            namespace: header.u64_at(0),
            limit: header.u32_at(8),
            pattern,
            resume: sections.get(3),
        })
    }

    fn to_body(&self) -> Result<Read, ProtocolError> {
        Ok(Read::new(
            self.namespace,
            self.limit,
            self.pattern.to_owned_pattern(),
            self.resume.to_vec(),
        ))
    }
}
