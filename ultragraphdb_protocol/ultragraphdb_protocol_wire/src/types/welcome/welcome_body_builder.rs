/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::welcome::Welcome;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::write_sections;

impl BodyBuilder for Welcome {
    const COMMAND: Command = Command::Welcome;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.protocol.to_le_bytes()),
            (4, &self.release.to_le_bytes()),
            (8, &self.max_frame.to_le_bytes()),
            (12, &self.max_streams.to_le_bytes()),
            (16, &self.initial_credit.to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        write_sections(&[], &[self.server.as_bytes()])
    }
}
