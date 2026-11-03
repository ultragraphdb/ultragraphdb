/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::entries::Entries;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::key_value_writer::write_entries;
use crate::utils::sections::{as_u32, write_sections};

impl BodyBuilder for Entries {
    const COMMAND: Command = Command::Entries;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &as_u32(self.entries.len()).to_le_bytes()),
            (4, &self.range.to_le_bytes()),
            (8, &[self.end.number()]),
            (16, &self.epoch.to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let entries = write_entries(
            self.entries
                .iter()
                .map(|(key, value)| (key.as_slice(), Some(value.as_slice()))),
        );
        write_sections(&[], &[&entries])
    }
}
