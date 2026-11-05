/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::marker::PhantomData;

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::begin::Begin;
use crate::types::begin_view::BeginView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::read_level::ReadLevel;
use crate::utils::fields::boolean;
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;
use crate::utils::token;

impl<'a> BodyView<'a> for BeginView<'a> {
    const COMMAND: Command = Command::Begin;

    type Body = Begin;

    /// Checks, in `PROTOCOL.md` §6.2's order: unused bytes 18–19 and 48–127 zero; `mode`; `level`; `read_version`
    /// nonzero only for a read-only transaction at level 0; the token; no sections.
    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 18), (20, 48)])?;
        let read_only = boolean(command, "mode", header.u8_at(16))?;
        let level_byte = header.u8_at(17);
        let level = ReadLevel::from_number(level_byte).ok_or(ProtocolError::field_value(
            command,
            "level",
            level_byte.into(),
        ))?;
        let read_version = header.u64_at(8);
        if read_version != 0 && !(read_only && level == ReadLevel::Snapshot) {
            return Err(ProtocolError::field_value(
                command,
                "read_version",
                read_version,
            ));
        }
        let token = token::read(command, header, 24)?;
        let _sections: Sections<'_, 0> = Sections::read(command, frame.body(), 0)?;
        Ok(Self {
            frame: PhantomData,
            tenant: header.u64_at(0),
            read_version,
            read_only,
            level,
            deadline_ms: header.u32_at(20),
            token,
        })
    }

    fn to_body(&self) -> Result<Begin, ProtocolError> {
        let begin = Begin::new(self.tenant, self.read_version, self.read_only)
            .with_level(self.level)
            .with_deadline_ms(self.deadline_ms);
        Ok(match &self.token {
            Some(token) => begin.with_token(token.clone()),
            None => begin,
        })
    }
}
