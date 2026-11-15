/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::ended::Ended;
use crate::types::ended_view::{EndedView, SESSION_SIZE};
use crate::types::frame_view::FrameView;
use crate::utils::body::read_sections;
use crate::utils::command_header::{CommandHeader, expect_command};
use crate::utils::fields::ascends;
use crate::utils::whole_records::whole_records;

impl<'a> EndedView<'a> {
    /// The body of `header` and `body`, checked as [`BodyView::from_frame`] states.
    pub(crate) fn read(header: CommandHeader<'a>, body: &'a [u8]) -> Result<Self, ProtocolError> {
        let command = Command::Ended.number();
        header.unused_zero(&[])?;
        let sections = read_sections::<1>(command, body, 0)?;
        let sessions = whole_records::<SESSION_SIZE>(command, 0, sections.get(0))?;
        if sessions.is_empty() {
            return Err(ProtocolError::count_mismatch(command, "sessions", 0, 1));
        }
        let mut before: Option<&[u8; SESSION_SIZE]> = None;
        for (index, session) in sessions.iter().enumerate() {
            if !ascends(session, before) {
                // Lossless: a `usize` is at most 64 bits.
                return Err(ProtocolError::field_value(command, "session", index as u64));
            }
            before = Some(session);
        }
        Ok(Self { sessions })
    }
}

impl<'a> BodyView<'a> for EndedView<'a> {
    const COMMAND: Command = Command::Ended;

    type Body = Ended;

    /// Checks, in `PROTOCOL.md` §13.25's order: command header bytes 0–127 zero; the sections; section 0 a whole
    /// number of 16 bytes ([`ProtocolError::section_length`]), at least one session
    /// ([`ProtocolError::count_mismatch`] `"sessions"`); each not all zero and above the one before, compared byte by
    /// byte ([`ProtocolError::field_value`] `"session"`, the session's index).
    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        Self::read(expect_command(frame, Self::COMMAND)?, frame.body())
    }

    fn to_body(&self) -> Result<Ended, ProtocolError> {
        Ok(Ended::new(self.sessions.to_vec()))
    }
}
