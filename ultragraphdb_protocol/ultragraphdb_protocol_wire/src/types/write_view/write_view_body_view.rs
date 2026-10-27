/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::string_list::StringList;
use crate::types::write::Write;
use crate::types::write_view::WriteView;
use crate::utils::changes::{check_records, decode_changeset};
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
        let sections = Sections::<4>::read(command, frame.body(), 0)?;
        let strings = StringList::read(command, (2, 3), sections.get(2), sections.get(3))?;
        let args = u32s(command, 1, sections.get(1))?;
        check_records(command, (0, 1, 3), sections.get(0), &args, &strings, false)?;
        Ok(Self {
            namespace: header.u64_at(0),
            records: sections.get(0),
            args,
            strings,
        })
    }

    fn to_body(&self) -> Result<Write, ProtocolError> {
        Ok(Write::new(decode_changeset(
            Self::COMMAND.number(),
            (0, 1, 3),
            self.namespace,
            self.records,
            &self.args,
            &self.strings,
        )?))
    }
}
