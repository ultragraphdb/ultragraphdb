/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::install::Install;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::write_sections;
use crate::utils::string_writer::StringWriter;

impl BodyBuilder for Install {
    const COMMAND: Command = Command::Install;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.tenant().to_le_bytes()),
            (8, &self.version().to_le_bytes()),
            (16, &self.epoch().to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let mut strings = StringWriter::default();
        for (name, text) in self.sources() {
            let _name: u32 = strings.push(name);
            let _text: u32 = strings.push(text);
        }
        let (ends, text) = strings.sections();
        write_sections(&[], &[&ends, &text, self.report().as_bytes()])
    }
}
