/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::closed::Closed;
use crate::types::command::Command;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::write_sections;

impl BodyBuilder for Closed {
    const COMMAND: Command = Command::Closed;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        [0; COMMAND_HEADER_SIZE]
    }

    fn body(&self) -> Vec<u8> {
        let partitions: Vec<u8> = self
            .partitions
            .iter()
            .flat_map(|partition| partition.encode())
            .collect();
        write_sections(&[], &[&partitions])
    }
}
