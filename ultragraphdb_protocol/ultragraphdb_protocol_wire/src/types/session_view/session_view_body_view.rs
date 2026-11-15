/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::marker::PhantomData;

use ultragraphdb_core_body::Sections;
use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::session::Session;
use crate::types::session_view::SessionView;
use crate::utils::body::read_sections;
use crate::utils::command_header::{CommandHeader, expect_command};
use crate::utils::fields::identity;

impl<'a> SessionView<'a> {
    /// The body of `header` and `body`, checked as [`BodyView::from_frame`] states.
    pub(crate) fn read(header: CommandHeader<'a>, body: &'a [u8]) -> Result<Self, ProtocolError> {
        let command = Command::Session.number();
        header.unused_zero(&[(0, 24)])?;
        let session = identity(header.0, 0);
        if session == [0; 16] {
            return Err(ProtocolError::field_value(command, "session", 0));
        }
        let _sections: Sections<'_, 0> = read_sections(command, body, 0)?;
        Ok(Self {
            frame: PhantomData,
            session,
            principal: header.u64_at(16),
        })
    }
}

impl<'a> BodyView<'a> for SessionView<'a> {
    const COMMAND: Command = Command::Session;

    type Body = Session;

    /// Checks, in `PROTOCOL.md` §13.23's order: unused bytes 24–127 zero; `session` not all zero
    /// ([`ProtocolError::field_value`] `"session"`); no sections.
    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        Self::read(expect_command(frame, Self::COMMAND)?, frame.body())
    }

    fn to_body(&self) -> Result<Session, ProtocolError> {
        Ok(Session::new(self.session, self.principal))
    }
}
