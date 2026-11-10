/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::register::Register;
use crate::types::register_view::RegisterView;
use crate::utils::body::read_sections;
use crate::utils::command_header::expect_command;
use crate::utils::fields::identity;
use std::marker::PhantomData;
use ultragraphdb_core_body::Sections;

impl<'a> BodyView<'a> for RegisterView<'a> {
    const COMMAND: Command = Command::Register;

    type Body = Register;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 16)])?;
        let _sections: Sections<'_, 0> = read_sections(command, frame.body(), 0)?;
        Ok(Self {
            frame: PhantomData,
            session: identity(header.0, 0),
        })
    }

    fn to_body(&self) -> Result<Register, ProtocolError> {
        Ok(Register::new(self.session))
    }
}
