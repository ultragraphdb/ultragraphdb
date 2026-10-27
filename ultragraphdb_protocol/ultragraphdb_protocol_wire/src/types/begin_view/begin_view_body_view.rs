/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::begin::Begin;
use crate::types::begin_view::BeginView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;
use std::marker::PhantomData;

impl<'a> BodyView<'a> for BeginView<'a> {
    const COMMAND: Command = Command::Begin;

    type Body = Begin;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 17)])?;
        let read_only = match header.u8_at(16) {
            0 => false,
            1 => true,
            mode => return Err(ProtocolError::FieldValue(command, "mode", mode.into())),
        };
        Sections::<0>::read(command, frame.body(), 0)?;
        Ok(Self {
            frame: PhantomData,
            namespace: header.u64_at(0),
            read_version: header.u64_at(8),
            read_only,
        })
    }

    fn to_body(&self) -> Result<Begin, ProtocolError> {
        Ok(Begin::new(
            self.namespace,
            self.read_version,
            self.read_only,
        ))
    }
}
