/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::credit::Credit;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::write_sections;

impl BodyBuilder for Credit {
    const COMMAND: Command = Command::Credit;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[(0, &self.frames.to_le_bytes())])
    }

    fn body(&self) -> Vec<u8> {
        write_sections(&[], &[])
    }
}
