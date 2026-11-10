/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::write_sections;

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::migrate::Migrate;
use crate::types::migrate_state::MigrateState;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;

impl BodyBuilder for Migrate {
    const COMMAND: Command = Command::Migrate;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        let state = self.state().map_or(0, MigrateState::number);
        command_header(&[
            (0, &self.tenant().to_le_bytes()),
            (8, &self.staged().to_le_bytes()),
            (16, &[self.action().number(), state]),
            (20, &self.refusal().to_le_bytes()),
            (24, &self.epoch().to_le_bytes()),
        ])
    }

    fn body(&self) -> Vec<u8> {
        write_sections(&[], &[self.message().as_bytes(), self.report().as_bytes()])
    }
}
