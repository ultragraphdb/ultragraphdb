/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::write_sections;

use crate::traits::body_builder::BodyBuilder;
use crate::types::bind::Bind;
use crate::types::command::Command;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;

impl BodyBuilder for Bind {
    const COMMAND: Command = Command::Bind;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.partition().to_le_bytes()),
            (4, &[self.kind().number()]),
            (8, &self.tenant().to_le_bytes()),
            (16, &self.read_version().to_le_bytes()),
            (24, &self.epoch().to_le_bytes()),
            (32, &self.principal().to_le_bytes()),
            (40, &self.transaction()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        write_sections(&[], &[])
    }
}
