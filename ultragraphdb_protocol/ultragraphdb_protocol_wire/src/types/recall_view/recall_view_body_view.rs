/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::marker::PhantomData;

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::recall::Recall;
use crate::types::recall_view::RecallView;
use crate::utils::fields::{at_least_one, identity};
use crate::utils::sections::{Sections, expect_command};

impl<'a> BodyView<'a> for RecallView<'a> {
    const COMMAND: Command = Command::Recall;

    type Body = Recall;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 4), (8, 32)])?;
        let partition = header.u32_at(0);
        if partition == 0 {
            return Err(ProtocolError::field_value(command, "partition", 0));
        }
        let session = identity(header.0, 8);
        if session == [0; 16] {
            return Err(ProtocolError::field_value(command, "session", 0));
        }
        let request = at_least_one(command, "request", header.u64_at(24))?;
        let _sections: Sections<'_, 0> = Sections::read(command, frame.body(), 0)?;
        Ok(Self {
            frame: PhantomData,
            partition,
            session,
            request,
        })
    }

    fn to_body(&self) -> Result<Recall, ProtocolError> {
        Ok(Recall::new(self.partition, self.session, self.request))
    }
}
