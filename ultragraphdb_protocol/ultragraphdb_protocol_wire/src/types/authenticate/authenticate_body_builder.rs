/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::authenticate::Authenticate;
use crate::types::command::Command;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::write_sections;

impl BodyBuilder for Authenticate {
    const COMMAND: Command = Command::Authenticate;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.step().to_le_bytes()),
            (4, &[u8::from(self.is_final())]),
            (8, &self.principal().to_le_bytes()),
            (16, &self.expires().to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        write_sections(&[], &[self.mechanism().as_bytes(), self.message()])
    }
}
