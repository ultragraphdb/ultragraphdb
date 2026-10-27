/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::begin::Begin;
use crate::types::command::Command;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::write_sections;

impl BodyBuilder for Begin {
    const COMMAND: Command = Command::Begin;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.namespace.to_le_bytes()),
            (8, &self.read_version.to_le_bytes()),
            (16, &[u8::from(self.read_only)]),
        ])
    }

    fn body(&self) -> Vec<u8> {
        write_sections(&[], &[])
    }
}
