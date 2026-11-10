/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::write_sections;

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::epoch_close::EpochClose;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;

impl BodyBuilder for EpochClose {
    const COMMAND: Command = Command::EpochClose;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.epoch.to_le_bytes()),
            (8, &self.time.to_le_bytes()),
            (16, &[u8::from(self.more)]),
            (24, &self.placement.to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let announcements: Vec<u8> = self.announcements.iter().flat_map(|a| a.encode()).collect();
        let evictions = self.evictions.concat();
        write_sections(&[], &[&announcements, &evictions])
    }
}
