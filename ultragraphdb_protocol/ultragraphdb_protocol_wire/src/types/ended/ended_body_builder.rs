/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::write_sections;
use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::ended::Ended;
use crate::types::ended_view::EndedView;
use crate::types::header::Header;
use crate::utils::checked_frame::checked_frame;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;

impl BodyBuilder for Ended {
    const COMMAND: Command = Command::Ended;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[])
    }

    fn body(&self) -> Vec<u8> {
        write_sections(&[], &[self.sessions().as_flattened()])
    }

    /// The frame of the sessions under `header`; a body [`crate::EndedView`] refuses is refused the same way: no
    /// session as [`ProtocolError::count_mismatch`] (`"sessions"`), a session of all zero or not above the one before
    /// as [`ProtocolError::field_value`] (`"session"`, its index).
    fn frame(&self, header: Header) -> Result<Vec<u8>, ProtocolError> {
        checked_frame(
            Self::COMMAND,
            &header,
            &self.command_header(),
            &self.body(),
            EndedView::read,
        )
    }
}
