/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::credit::Credit;
use crate::types::credit_view::CreditView;
use crate::types::frame_view::FrameView;
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;
use std::marker::PhantomData;

impl<'a> BodyView<'a> for CreditView<'a> {
    const COMMAND: Command = Command::Credit;

    type Body = Credit;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 4)])?;
        let frames = header.u32_at(0);
        if frames == 0 {
            return Err(ProtocolError::FieldValue(command, "frames", 0));
        }
        Sections::<0>::read(command, frame.body(), 0)?;
        Ok(Self {
            frame: PhantomData,
            frames,
        })
    }

    fn to_body(&self) -> Result<Credit, ProtocolError> {
        Ok(Credit::new(self.frames))
    }
}
