/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::lease::Lease;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::write_sections;

impl BodyBuilder for Lease {
    const COMMAND: Command = Command::Lease;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.type_prefix.to_le_bytes()),
            (4, &self.partition.to_le_bytes()),
            (8, &self.size.to_le_bytes()),
            (16, &self.start.to_le_bytes()),
            (24, &self.end.to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        write_sections(&[], &[])
    }
}
