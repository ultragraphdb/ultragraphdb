/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::cancel::Cancel;
use crate::types::cancel_view::CancelView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;
use std::marker::PhantomData;

impl<'a> BodyView<'a> for CancelView<'a> {
    const COMMAND: Command = Command::Cancel;

    type Body = Cancel;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[])?;
        Sections::<0>::read(command, frame.body(), 0)?;
        Ok(Self { frame: PhantomData })
    }

    fn to_body(&self) -> Result<Cancel, ProtocolError> {
        Ok(Cancel)
    }
}
