/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::write_sections;
use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_builder::BodyBuilder;
use crate::types::check::Check;
use crate::types::check_view::{CHECK_SIZE, CheckView};
use crate::types::command::Command;
use crate::types::header::Header;
use crate::utils::checked_frame::checked_frame;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;

impl BodyBuilder for Check {
    const COMMAND: Command = Command::Check;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[])
    }

    fn body(&self) -> Vec<u8> {
        let mut section = Vec::with_capacity(CHECK_SIZE.saturating_mul(self.checks().len()));
        for check in self.checks() {
            section.extend_from_slice(&check.session());
            section.extend_from_slice(&check.principal().to_le_bytes());
            section.extend_from_slice(&[check.kind().number(), check.outcome().number()]);
            section.extend_from_slice(&[0; 6]);
        }
        write_sections(&[], &[&section])
    }

    /// The frame of the checks under `header`; a body [`crate::CheckView`] refuses is refused the same way: no check as
    /// [`ProtocolError::count_mismatch`] (`"checks"`), a `session` of all zero or not above the one before as
    /// [`ProtocolError::field_value`] (`"check"`, its index).
    fn frame(&self, header: Header) -> Result<Vec<u8>, ProtocolError> {
        checked_frame(
            Self::COMMAND,
            &header,
            &self.command_header(),
            &self.body(),
            CheckView::read,
        )
    }
}
