/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::write::Write;
use crate::utils::changes::ChangeWriter;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::u32_bytes;
use crate::utils::sections::write_sections;

impl BodyBuilder for Write {
    const COMMAND: Command = Command::Write;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[(0, &self.changeset().namespace().to_le_bytes())])
    }

    fn body(&self) -> Vec<u8> {
        let mut writer = ChangeWriter::default();
        for change in self.changeset().changes() {
            writer.change(change);
        }
        let (ends, text) = writer.strings.sections();
        write_sections(
            &[],
            &[&writer.records, &u32_bytes(&writer.args), &ends, &text],
        )
    }
}
