/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::read::Read;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::write_sections;
use crate::utils::string_writer::StringWriter;

impl BodyBuilder for Read {
    const COMMAND: Command = Command::Read;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.namespace.to_le_bytes()),
            (8, &self.limit.to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let mut terms = StringWriter::default();
        let slots = self.pattern().write(&mut terms);
        let (ends, text) = terms.sections();
        write_sections(&[], &[&slots, &ends, &text, self.resume()])
    }
}
