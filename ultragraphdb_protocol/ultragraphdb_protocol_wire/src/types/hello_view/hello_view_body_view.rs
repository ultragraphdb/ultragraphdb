/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::hello::Hello;
use crate::types::hello_view::HelloView;
use crate::utils::constants::{MAX_FRAME_FLOOR, MAX_FRAME_SIZE};
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;
use crate::utils::sections::text;

impl<'a> BodyView<'a> for HelloView<'a> {
    const COMMAND: Command = Command::Hello;

    type Body = Hello;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 12)])?;
        let protocol_min = header.u16_at(0);
        let protocol_max = header.u16_at(2);
        if protocol_min == 0 {
            return Err(ProtocolError::FieldValue(command, "protocol_min", 0));
        }
        if protocol_max < protocol_min {
            return Err(ProtocolError::FieldValue(
                command,
                "protocol_max",
                protocol_max.into(),
            ));
        }
        let max_frame = header.u32_at(8);
        if !(MAX_FRAME_FLOOR..=MAX_FRAME_SIZE).contains(&(max_frame as usize)) {
            return Err(ProtocolError::FieldValue(
                command,
                "max_frame",
                max_frame.into(),
            ));
        }
        let sections = Sections::<1>::read(command, frame.body(), 0)?;
        Ok(Self {
            protocol_min,
            protocol_max,
            release: header.u32_at(4),
            max_frame,
            client: text(command, 0, sections.get(0))?,
        })
    }

    fn to_body(&self) -> Result<Hello, ProtocolError> {
        Ok(Hello::new(
            self.protocol_min,
            self.protocol_max,
            self.release,
            self.max_frame,
            self.client,
        ))
    }
}
