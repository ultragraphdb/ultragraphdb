/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::marker::PhantomData;

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::close_request::CloseRequest;
use crate::types::close_request_view::CloseRequestView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::utils::fields::{at_least_one, epoch};
use crate::utils::sections::{Sections, expect_command};

impl<'a> BodyView<'a> for CloseRequestView<'a> {
    const COMMAND: Command = Command::CloseRequest;

    type Body = CloseRequest;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 4), (8, 16)])?;
        let partition = header.u32_at(0);
        let _partition: u64 = at_least_one(command, "partition", partition.into())?;
        let open = epoch(command, "epoch", header.u64_at(8), false)?;
        let _sections: Sections<'_, 0> = Sections::read(command, frame.body(), 0)?;
        Ok(Self {
            frame: PhantomData,
            partition,
            epoch: open,
        })
    }

    fn to_body(&self) -> Result<CloseRequest, ProtocolError> {
        Ok(CloseRequest::new(self.partition, self.epoch))
    }
}
