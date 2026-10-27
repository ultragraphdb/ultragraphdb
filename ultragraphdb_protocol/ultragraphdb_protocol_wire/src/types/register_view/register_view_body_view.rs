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
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;
use std::marker::PhantomData;

impl<'a> BodyView<'a> for RegisterView<'a> {
    const COMMAND: Command = Command::Register;

    type Body = Register;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 8)])?;
        Sections::<0>::read(command, frame.body(), 0)?;
        Ok(Self {
            frame: PhantomData,
            session: header.u64_at(0),
        })
    }

    fn to_body(&self) -> Result<Register, ProtocolError> {
        Ok(Register::new(self.session))
    }
}
