/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::hello::Hello;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::write_sections;

impl BodyBuilder for Hello {
    const COMMAND: Command = Command::Hello;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.protocol_min.to_le_bytes()),
            (2, &self.protocol_max.to_le_bytes()),
            (4, &self.release.to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        write_sections(&[], &[self.client.as_bytes()])
    }
}
