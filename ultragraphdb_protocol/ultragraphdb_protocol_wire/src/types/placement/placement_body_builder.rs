/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::write_sections;

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::placement::Placement;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;

impl BodyBuilder for Placement {
    const COMMAND: Command = Command::Placement;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[
            (0, &self.position.to_le_bytes()),
            (8, &self.epoch.to_le_bytes()),
            (16, &self.from.to_le_bytes()),
            (24, &self.until.to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        let ranges: Vec<u8> = self.ranges.iter().flat_map(|r| r.encode()).collect();
        let runs: Vec<u8> = self.runs.iter().flat_map(|r| r.encode()).collect();
        let leaders: Vec<u8> = self.leaders.iter().flat_map(|l| l.encode()).collect();
        write_sections(&[], &[&ranges, &runs, &leaders])
    }
}
