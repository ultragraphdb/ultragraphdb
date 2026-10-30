/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::authenticate::Authenticate;
use crate::types::authenticate_view::AuthenticateView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::utils::sections::Sections;
use crate::utils::sections::expect_command;
use crate::utils::sections::text;

impl<'a> BodyView<'a> for AuthenticateView<'a> {
    const COMMAND: Command = Command::Authenticate;

    type Body = Authenticate;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 5), (8, 24)])?;
        let step = header.u32_at(0);
        let last = header.u8_at(4);
        let principal = header.u64_at(8);
        let expires = header.u64_at(16);
        if last > 1 {
            return Err(ProtocolError::FieldValue(command, "final", last.into()));
        }
        let last = last == 1;
        // A final message is the server's: an odd step.
        if last && step % 2 == 0 {
            return Err(ProtocolError::FieldValue(command, "step", step.into()));
        }
        if !last && principal != 0 {
            return Err(ProtocolError::FieldValue(command, "principal", principal));
        }
        if !last && expires != 0 {
            return Err(ProtocolError::FieldValue(command, "expires", expires));
        }
        let sections = Sections::<2>::read(command, frame.body(), 0)?;
        let mechanism = text(command, 0, sections.get(0))?;
        if (step == 0) == mechanism.is_empty() {
            return Err(ProtocolError::FieldValue(
                command,
                "mechanism",
                mechanism.len() as u64,
            ));
        }
        Ok(Self {
            step,
            last,
            principal,
            expires,
            mechanism,
            message: sections.get(1),
        })
    }

    fn to_body(&self) -> Result<Authenticate, ProtocolError> {
        let message = Authenticate::new(self.step, self.mechanism, self.message.to_vec());
        Ok(if self.last {
            message.with_final(self.principal, self.expires)
        } else {
            message
        })
    }
}
