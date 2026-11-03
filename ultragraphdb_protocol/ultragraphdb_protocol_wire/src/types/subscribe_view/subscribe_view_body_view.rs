/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::marker::PhantomData;

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::subscribe::Subscribe;
use crate::types::subscribe_view::SubscribeView;
use crate::utils::fields::epoch;
use crate::utils::sections::{Sections, expect_command};

impl<'a> BodyView<'a> for SubscribeView<'a> {
    const COMMAND: Command = Command::Subscribe;

    type Body = Subscribe;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 8)])?;
        let from = epoch(command, "from", header.u64_at(0), true)?;
        let _sections: Sections<'_, 0> = Sections::read(command, frame.body(), 0)?;
        Ok(Self {
            frame: PhantomData,
            from,
        })
    }

    fn to_body(&self) -> Result<Subscribe, ProtocolError> {
        Ok(Subscribe::new(self.from))
    }
}
