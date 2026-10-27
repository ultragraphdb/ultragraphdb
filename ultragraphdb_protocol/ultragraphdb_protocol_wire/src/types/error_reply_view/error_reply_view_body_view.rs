/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::error_code::ErrorCode;
use crate::types::error_reply::ErrorReply;
use crate::types::error_reply_view::ErrorReplyView;
use crate::types::frame_view::FrameView;
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;
use crate::utils::sections::text;

impl<'a> BodyView<'a> for ErrorReplyView<'a> {
    const COMMAND: Command = Command::Error;

    type Body = ErrorReply;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 2), (8, 16)])?;
        let number = header.u16_at(0);
        let code = ErrorCode::from_number(number).ok_or(ProtocolError::FieldValue(
            command,
            "code",
            number.into(),
        ))?;
        let sections = Sections::<2>::read(command, frame.body(), 0)?;
        let value = text(command, 1, sections.get(1))?;
        Ok(Self {
            code,
            detail: header.u64_at(8),
            message: text(command, 0, sections.get(0))?,
            value: (!value.is_empty()).then_some(value),
        })
    }

    fn to_body(&self) -> Result<ErrorReply, ProtocolError> {
        let reply = ErrorReply::new(self.code, self.message).with_detail(self.detail);
        Ok(match self.value {
            Some(value) => reply.with_value(value),
            None => reply,
        })
    }
}
