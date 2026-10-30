/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::execute::Execute;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::u32_bytes;
use crate::utils::sections::write_sections;

impl BodyBuilder for Execute {
    const COMMAND: Command = Command::Execute;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.statement().to_le_bytes()),
            (8, &self.transaction().to_le_bytes()),
            (16, &self.limit().to_le_bytes()),
            (20, &u32::from(self.statement_ids()).to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let (records, text) = self.terms().sections();
        write_sections(
            &[],
            &[
                &records,
                &text,
                &u32_bytes(self.parameters()),
                self.resume(),
            ],
        )
    }
}
