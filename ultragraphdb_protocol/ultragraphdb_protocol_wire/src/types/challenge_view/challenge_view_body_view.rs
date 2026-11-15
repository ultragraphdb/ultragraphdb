/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::challenge::Challenge;
use crate::types::challenge_view::ChallengeView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::utils::body::{read_sections, text};
use crate::utils::command_header::{CommandHeader, expect_command};

/// The fewest iterations a reply names.
const ITERATIONS_MIN: u32 = 4_096;

/// The most iterations a reply names.
const ITERATIONS_MAX: u32 = 10_000_000;

impl<'a> ChallengeView<'a> {
    /// The body of `header` and `body`, checked in `PROTOCOL.md` §13.8's order: unused bytes 4–127 zero; `iterations`
    /// 0 in a request and 4,096 to 10,000,000 in a reply; the sections; section 0 UTF-8; section 1 empty in a request
    /// and not empty in a reply.
    pub(crate) fn read(header: CommandHeader<'a>, body: &'a [u8]) -> Result<Self, ProtocolError> {
        let command = Command::Challenge.number();
        header.unused_zero(&[(0, 4)])?;
        let iterations = header.u32_at(0);
        let request = iterations == 0;
        if !request && !(ITERATIONS_MIN..=ITERATIONS_MAX).contains(&iterations) {
            return Err(ProtocolError::field_value(
                command,
                "iterations",
                iterations.into(),
            ));
        }
        let sections = read_sections::<2>(command, body, 0)?;
        let name = text(command, 0, sections.get(0))?;
        let salt = sections.get(1);
        match (request, salt.is_empty()) {
            (true, false) => Err(ProtocolError::count_mismatch(
                command,
                "salt",
                salt.len() as u64, // lossless: a `usize` is at most 64 bits
                0,
            )),
            (false, true) => Err(ProtocolError::count_mismatch(command, "salt", 0, 1)),
            (true, true) | (false, false) => Ok(Self {
                name,
                iterations,
                salt,
            }),
        }
    }
}

impl<'a> BodyView<'a> for ChallengeView<'a> {
    const COMMAND: Command = Command::Challenge;

    type Body = Challenge;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        Self::read(expect_command(frame, Self::COMMAND)?, frame.body())
    }

    fn to_body(&self) -> Result<Challenge, ProtocolError> {
        let request = Challenge::request(self.name);
        Ok(if self.iterations == 0 {
            request
        } else {
            request.with_salt(self.iterations, self.salt.to_vec())
        })
    }
}
