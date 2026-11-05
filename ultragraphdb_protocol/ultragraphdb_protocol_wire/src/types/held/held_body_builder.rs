/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::body_builder::BodyBuilder;
use crate::types::command::Command;
use crate::types::held::Held;
use crate::utils::command_header::command_header;
use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::sections::write_sections;

impl BodyBuilder for Held {
    const COMMAND: Command = Command::Held;

    fn command_header(&self) -> [u8; COMMAND_HEADER_SIZE] {
        command_header(&[])
    }

    fn body(&self) -> Vec<u8> {
        write_sections(&[], &[])
    }
}
