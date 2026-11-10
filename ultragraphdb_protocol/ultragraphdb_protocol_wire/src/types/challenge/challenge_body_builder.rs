/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::write_sections;

use crate::traits::body_builder::BodyBuilder;
use crate::types::challenge::Challenge;
use crate::types::command::Command;
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
}
