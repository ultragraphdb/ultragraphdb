/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::abort::Abort;
use crate::types::abort_view::AbortView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;
use std::marker::PhantomData;

impl<'a> BodyView<'a> for AbortView<'a> {
    const COMMAND: Command = Command::Abort;

    type Body = Abort;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[])?;
        Sections::<0>::read(command, frame.body(), 0)?;
        Ok(Self { frame: PhantomData })
    }

    fn to_body(&self) -> Result<Abort, ProtocolError> {
        Ok(Abort)
    }
}
