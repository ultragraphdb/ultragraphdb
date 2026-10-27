/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::welcome::Welcome;
use crate::types::welcome_view::WelcomeView;
use crate::utils::constants::{CHUNK_SIZE, MAX_FRAME_SIZE};
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;
use crate::utils::sections::text;

impl<'a> BodyView<'a> for WelcomeView<'a> {
    const COMMAND: Command = Command::Welcome;

    type Body = Welcome;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 2), (4, 20)])?;
        let protocol = header.u16_at(0);
        let max_frame = header.u32_at(8);
        let max_streams = header.u32_at(12);
        if protocol == 0 {
            return Err(ProtocolError::FieldValue(command, "protocol", 0));
        }
        if !(CHUNK_SIZE..=MAX_FRAME_SIZE).contains(&(max_frame as usize)) {
            return Err(ProtocolError::FieldValue(
                command,
                "max_frame",
                max_frame.into(),
            ));
        }
        if max_streams == 0 {
            return Err(ProtocolError::FieldValue(command, "max_streams", 0));
        }
        let sections = Sections::<1>::read(command, frame.body(), 0)?;
        Ok(Self {
            protocol,
            release: header.u32_at(4),
            max_frame,
            max_streams,
            initial_credit: header.u32_at(16),
            server: text(command, 0, sections.get(0))?,
        })
    }

    fn to_body(&self) -> Result<Welcome, ProtocolError> {
        Ok(Welcome::new(
            self.protocol,
            self.release,
            self.max_frame,
            self.max_streams,
            self.initial_credit,
            self.server,
        ))
    }
}
