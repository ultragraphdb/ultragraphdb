/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::compute_read::ComputeRead;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::write_sections;

impl BodyBuilder for ComputeRead {
    const COMMAND: Command = Command::ComputeRead;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.partition().to_le_bytes()),
            (4, &[u8::from(self.read_only())]),
            (5, &[self.level().number()]),
            (6, &[u8::from(self.named())]),
            (8, &self.tenant().to_le_bytes()),
            (16, &self.read_version().to_le_bytes()),
            (24, &self.epoch().to_le_bytes()),
            (32, &self.principal().to_le_bytes()),
            (40, &self.transaction()),
            (56, &self.limit().to_le_bytes()),
            (60, &self.deadline_ms().to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let (records, text) = self.terms().sections();
        write_sections(
            &[],
            &[&self.pattern().write(), &records, &text, self.resume()],
        )
    }
}
