/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::welcome::Welcome;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::write_sections;
use crate::utils::string_writer::StringWriter;

impl BodyBuilder for Welcome {
    const COMMAND: Command = Command::Welcome;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.protocol().to_le_bytes()),
            (2, &[u8::from(self.authenticates())]),
            (4, &self.release().to_le_bytes()),
            (8, &self.max_frame().to_le_bytes()),
            (12, &self.max_streams().to_le_bytes()),
            (16, &self.initial_credit().to_le_bytes()),
            (20, &self.max_write_set().to_le_bytes()),
            (24, &self.transaction_ms().to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let mut mechanisms = StringWriter::default();
        for mechanism in self.mechanisms() {
            mechanisms.push(mechanism);
        }
        let (ends, text) = mechanisms.sections();
        write_sections(&[], &[self.server().as_bytes(), &ends, &text])
    }
}
