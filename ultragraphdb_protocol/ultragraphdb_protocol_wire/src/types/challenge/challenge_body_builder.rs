/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::write_sections;
use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_builder::BodyBuilder;
use crate::types::challenge::Challenge;
use crate::types::challenge_view::ChallengeView;
use crate::types::command::Command;
use crate::types::header::Header;
use crate::utils::checked_frame::checked_frame;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;

impl BodyBuilder for Challenge {
    const COMMAND: Command = Command::Challenge;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[(0, &self.iterations().to_le_bytes())])
    }

    fn body(&self) -> Vec<u8> {
        write_sections(&[], &[self.name().as_bytes(), self.salt()])
    }

    /// The frame of the body under `header`; a body [`crate::ChallengeView`] refuses is refused the same way.
    fn frame(&self, header: Header) -> Result<Vec<u8>, ProtocolError> {
        checked_frame(
            Self::COMMAND,
            &header,
            &self.command_header(),
            &self.body(),
            ChallengeView::read,
        )
    }
}
