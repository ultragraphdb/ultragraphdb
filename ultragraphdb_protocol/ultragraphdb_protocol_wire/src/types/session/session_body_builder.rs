/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::write_sections;
use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::header::Header;
use crate::types::session::Session;
use crate::types::session_view::SessionView;
use crate::utils::checked_frame::checked_frame;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;

impl BodyBuilder for Session {
    const COMMAND: Command = Command::Session;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[(0, &self.session()), (16, &self.principal().to_le_bytes())])
    }

    fn body(&self) -> Vec<u8> {
        write_sections(&[], &[])
    }

    /// The frame of the registration under `header`; a body [`crate::SessionView`] refuses is refused the same way:
    /// a `session` of all zero as [`ProtocolError::field_value`] (`"session"`).
    fn frame(&self, header: Header) -> Result<Vec<u8>, ProtocolError> {
        checked_frame(
            Self::COMMAND,
            &header,
            &self.command_header(),
            &self.body(),
            SessionView::read,
        )
    }
}
